use crate::domain::{
    Plugin, PluginMetadata, PluginStatus, PluginInfo,
    EventBus, Event, Hook, Profile,
};
use crate::errors::{ShellBeError, Result, ErrorContext};
use crate::utils::{FileLock, ensure_directory, system_requirements::SystemRequirements, plugin_security::PluginSecurityValidator};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use libloading::{Library, Symbol};
use reqwest::blocking::Client;
use std::fs;
use std::io::{self, Write};
use std::collections::HashSet;
use chrono::Utc;
use tokio::sync::RwLock;

/// Repository for managing plugin metadata
#[async_trait::async_trait]
pub trait PluginRepository: Send + Sync {
    /// Get plugin metadata by name
    async fn get(&self, name: &str) -> Result<Option<PluginMetadata>>;

    /// List all plugins
    async fn list(&self) -> Result<Vec<PluginMetadata>>;

    /// Save plugin metadata
    async fn save(&self, metadata: PluginMetadata) -> Result<()>;

    /// Remove plugin metadata
    async fn remove(&self, name: &str) -> Result<()>;

    /// Update plugin status
    async fn update_status(&self, name: &str, status: PluginStatus) -> Result<()>;
}

/// Plugin sandbox settings for security
#[derive(Debug, Clone)]
pub struct PluginSandboxSettings {
    /// Whether to enable file system access
    pub allow_fs_access: bool,
    /// Whether to enable network access
    pub allow_network_access: bool,
    /// Maximum memory usage (in bytes)
    pub max_memory_bytes: Option<usize>,
    /// Allowed executable paths
    pub allowed_executables: Vec<PathBuf>,
    /// Allowed environment variables
    pub allowed_env_vars: Vec<String>,
}

impl Default for PluginSandboxSettings {
    fn default() -> Self {
        Self {
            allow_fs_access: true,
            allow_network_access: false,
            max_memory_bytes: Some(50 * 1024 * 1024), // 50MB
            allowed_executables: vec![],
            allowed_env_vars: vec![
                "HOME".to_string(),
                "PATH".to_string(),
                "USER".to_string(),
                "SHELL".to_string(),
            ],
        }
    }
}

/// Service for managing plugins
pub struct PluginService {
    repository: Arc<dyn PluginRepository>,
    event_bus: Arc<EventBus>,
    plugins_dir: PathBuf,
    loaded_plugins: Arc<RwLock<Vec<(String, Arc<dyn Plugin>, Arc<Library>)>>>,
    sandbox_settings: PluginSandboxSettings,
    security_validator: PluginSecurityValidator,
    system_requirements: SystemRequirements,
}

impl PluginService {
    /// Create a new plugin service
    pub fn new(
        repository: Arc<dyn PluginRepository>,
        event_bus: Arc<EventBus>,
        plugins_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            repository,
            event_bus,
            plugins_dir: plugins_dir.into(),
            loaded_plugins: Arc::new(RwLock::new(Vec::new())),
            sandbox_settings: PluginSandboxSettings::default(),
            security_validator: PluginSecurityValidator::default(),
            system_requirements: SystemRequirements::default(),
        }
    }

    /// Initialize the plugin system and load enabled plugins
    pub async fn initialize(&self) -> Result<()> {
        // Check system requirements
        self.system_requirements.all_requirements_met()
            .with_context(|| "Failed to initialize plugin system: system requirements not met".to_string())?;

        // Ensure plugins directory exists
        ensure_directory(&self.plugins_dir).await
            .with_context(|| format!("Failed to create plugins directory: {}", self.plugins_dir.display()))?;

        // Load enabled plugins
        let plugins = self.repository.list().await?;

        for metadata in plugins {
            if metadata.status == PluginStatus::Enabled {
                match self.load_plugin_internal(&metadata.info.name, &metadata.path).await {
                    Ok(_) => {
                        tracing::info!("Loaded plugin: {}", metadata.info.name);
                    }
                    Err(e) => {
                        tracing::error!("Failed to load plugin {}: {}", metadata.info.name, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// List all installed plugins
    pub async fn list_plugins(&self) -> Result<Vec<PluginMetadata>> {
        self.repository.list().await
    }

    /// Get metadata for a specific plugin
    pub async fn get_plugin(&self, name: &str) -> Result<PluginMetadata> {
        match self.repository.get(name).await? {
            Some(metadata) => Ok(metadata),
            None => Err(ShellBeError::NotFound(format!("Plugin not found: {}", name))),
        }
    }

    /// Set custom sandbox settings
    pub fn set_sandbox_settings(&mut self, settings: PluginSandboxSettings) {
        self.sandbox_settings = settings;
    }

    /// Install a plugin from a GitHub URL
    pub async fn install_from_github(&self, github_url: &str) -> Result<PluginMetadata> {
        // Parse GitHub URL
        let (owner, repo) = parse_github_url(github_url)
            .with_context(|| format!("Failed to parse GitHub URL: {}", github_url))?;

        // Create plugin directory path
        let plugin_dir = self.plugins_dir.join(&repo);

        // Acquire a lock for installation
        let lock_path = plugin_dir.with_extension("lock");
        let mut lock = FileLock::new(&lock_path).await;

        if !lock.acquire(10000).await? {
            return Err(ShellBeError::Security(format!(
                "Failed to acquire lock for plugin installation: {}", repo
            )));
        }

        // Create temporary directory
        let temp_dir = tempfile::tempdir()
            .map_err(|e| ShellBeError::Io(format!("Failed to create temporary directory: {}", e)))?;
        let zip_path = temp_dir.path().join(format!("{}.zip", repo));

        // Download the zip file
        let download_url = format!("https://github.com/{}/{}/archive/main.zip", owner, repo);

        tracing::info!("Downloading plugin from {}", download_url);

        let client = Client::new();
        let mut response = client.get(&download_url).send()
            .map_err(|e| ShellBeError::Update(format!("Failed to download plugin: {}", e)))?;

        if !response.status().is_success() {
            return Err(ShellBeError::Update(format!("HTTP error: {}", response.status())));
        }

        // Save the zip file
        let mut file = fs::File::create(&zip_path)
            .map_err(|e| ShellBeError::Io(format!("Failed to create zip file: {}", e)))?;
        
        response.copy_to(&mut file)
            .map_err(|e| ShellBeError::Io(format!("Failed to save zip content: {}", e)))?;

        // Extract the zip file
        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir_all(&extract_dir)
            .map_err(|e| ShellBeError::Io(format!("Failed to create extraction directory: {}", e)))?;

        tracing::info!("Extracting plugin archive");

        let file = fs::File::open(&zip_path)
            .map_err(|e| ShellBeError::Io(format!("Failed to open zip file: {}", e)))?;
        
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| ShellBeError::Plugin(format!("Failed to read zip archive: {}", e)))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| ShellBeError::Plugin(format!("Failed to extract zip entry: {}", e)))?;
            
            let outpath = extract_dir.join(file.name());

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)
                    .map_err(|e| ShellBeError::Io(format!("Failed to create directory: {}", e)))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| ShellBeError::Io(format!("Failed to create parent directory: {}", e)))?;
                }
                let mut outfile = fs::File::create(&outpath)
                    .map_err(|e| ShellBeError::Io(format!("Failed to create file: {}", e)))?;
                
                io::copy(&mut file, &mut outfile)
                    .map_err(|e| ShellBeError::Io(format!("Failed to write file: {}", e)))?;
            }
        }

        // Find the plugin directory
        let plugin_root = extract_dir.join(format!("{}-main", repo));

        // Check if plugin.info exists
        let plugin_info_path = plugin_root.join("plugin.info");
        if !plugin_info_path.exists() {
            return Err(ShellBeError::Plugin("Missing plugin.info file".to_string()));
        }

        // Read plugin info
        let plugin_info = fs::read_to_string(plugin_info_path)
            .map_err(|e| ShellBeError::Io(format!("Failed to read plugin.info: {}", e)))?;
        
        let mut name = None;
        let mut version = None;
        let mut description = None;
        let mut author = None;
        let mut api_version = None;

        for line in plugin_info.lines() {
            if let Some(value) = line.strip_prefix("NAME=") {
                name = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("VERSION=") {
                version = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("DESCRIPTION=") {
                description = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("AUTHOR=") {
                author = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("API_VERSION=") {
                api_version = Some(value.to_string());
            }
        }

        let plugin_name = name.unwrap_or_else(|| repo.clone());
        let plugin_version = version.unwrap_or_else(|| "0.1.0".to_string());
        let plugin_description = description.unwrap_or_else(|| "No description".to_string());
        let plugin_author = author.unwrap_or_else(|| owner.clone());
        let plugin_api_version = api_version.unwrap_or_else(|| "2.0.0".to_string());

        // Verify API version compatibility
        if plugin_api_version != "2.0.0" {
            return Err(ShellBeError::Plugin("Plugin API version mismatch".to_string()));
        }

        // Check if plugin already exists
        if let Some(_) = self.repository.get(&plugin_name).await? {
            return Err(ShellBeError::AlreadyExists(format!("Plugin already exists: {}", plugin_name)));
        }

        // Create plugin directory
        fs::create_dir_all(&plugin_dir)
            .map_err(|e| ShellBeError::Io(format!("Failed to create plugin directory: {}", e)))?;

        // Copy plugin files
        copy_dir_all(&plugin_root, &plugin_dir)
            .map_err(|e| ShellBeError::Io(format!("Failed to copy plugin files: {}", e)))?;

        // Find the library file
        let lib_path = find_plugin_library(&plugin_dir)
            .with_context(|| format!("Failed to find plugin library in {}", plugin_dir.display()))?;

        // Validate plugin security
        self.security_validator.validate(&lib_path)
            .with_context(|| format!("Plugin security validation failed for {}", lib_path.display()))?;

        // Create metadata
        let metadata = PluginMetadata {
            info: PluginInfo {
                name: plugin_name.clone(),
                version: plugin_version,
                description: plugin_description,
                author: plugin_author,
                source_url: Some(github_url.to_string()),
            },
            status: PluginStatus::Disabled,
            path: plugin_dir,
            installed_at: Utc::now(),
            updated_at: None,
        };

        // Save metadata
        self.repository.save(metadata.clone()).await?;

        // Release the lock
        lock.release().await?;

        tracing::info!("Plugin '{}' installed successfully", plugin_name);

        // Return the metadata
        Ok(metadata)
    }

    /// Enable a plugin
    pub async fn enable_plugin(&self, name: &str) -> Result<()> {
        // Get plugin metadata
        let metadata = match self.repository.get(name).await? {
            Some(metadata) => metadata,
            None => return Err(ShellBeError::NotFound(format!("Plugin not found: {}", name))),
        };

        // Check if already enabled
        if metadata.status == PluginStatus::Enabled {
            return Ok(());
        }

        // Load the plugin
        self.load_plugin_internal(name, &metadata.path).await?;

        // Update status
        self.repository.update_status(name, PluginStatus::Enabled).await?;

        // Run plugin hooks
        let plugin = self.get_loaded_plugin(name).await?;
        if let Err(e) = plugin.on_enable().await {
            tracing::warn!("Error in plugin.on_enable: {}", e);
        }

        // Also run the plugin enabled hook
        if let Err(e) = plugin.execute_hook(Hook::PluginEnabled, None).await {
            tracing::warn!("Error in plugin PluginEnabled hook: {}", e);
        }

        // Publish event
        self.event_bus.publish(Event::PluginEnabled(name.to_string()));

        tracing::info!("Plugin '{}' enabled", name);

        Ok(())
    }

    /// Disable a plugin
    pub async fn disable_plugin(&self, name: &str) -> Result<()> {
        // Get plugin metadata
        let metadata = match self.repository.get(name).await? {
            Some(metadata) => metadata,
            None => return Err(ShellBeError::NotFound(format!("Plugin not found: {}", name))),
        };

        // Check if already disabled
        if metadata.status == PluginStatus::Disabled {
            return Ok(());
        }

        // Run plugin hooks
        if let Ok(plugin) = self.get_loaded_plugin(name).await {
            if let Err(e) = plugin.on_disable().await {
                tracing::warn!("Error in plugin.on_disable: {}", e);
            }

            // Also run the plugin disabled hook
            if let Err(e) = plugin.execute_hook(Hook::PluginDisabled, None).await {
                tracing::warn!("Error in plugin PluginDisabled hook: {}", e);
            }
        }

        // Update status
        self.repository.update_status(name, PluginStatus::Disabled).await?;

        // Unload the plugin
        self.unload_plugin(name).await?;

        // Publish event
        self.event_bus.publish(Event::PluginDisabled(name.to_string()));

        tracing::info!("Plugin '{}' disabled", name);

        Ok(())
    }

    /// Remove a plugin
    pub async fn remove_plugin(&self, name: &str) -> Result<()> {
        // Get plugin metadata
        let metadata = match self.repository.get(name).await? {
            Some(metadata) => metadata,
            None => return Err(ShellBeError::NotFound(format!("Plugin not found: {}", name))),
        };

        // Disable the plugin if it's enabled
        if metadata.status == PluginStatus::Enabled {
            self.disable_plugin(name).await?;
        }

        // Remove plugin directory
        if metadata.path.exists() {
            fs::remove_dir_all(&metadata.path)
                .map_err(|e| ShellBeError::Io(format!("Failed to remove plugin directory: {}", e)))?;
        }

        // Remove metadata
        self.repository.remove(name).await?;

        tracing::info!("Plugin '{}' removed", name);

        Ok(())
    }

    /// Update a plugin from GitHub
    pub async fn update_plugin(&self, name: &str) -> Result<PluginMetadata> {
        // Get plugin metadata
        let metadata = match self.repository.get(name).await? {
            Some(metadata) => metadata,
            None => return Err(ShellBeError::NotFound(format!("Plugin not found: {}", name))),
        };

        // Check if we have a source URL
        let source_url = metadata.info.source_url.clone().ok_or_else(|| {
            ShellBeError::Plugin("No source URL available for update".to_string())
        })?;

        // Save the plugin status
        let was_enabled = metadata.status == PluginStatus::Enabled;

        // Disable the plugin if it's enabled
        if was_enabled {
            self.disable_plugin(name).await?;
        }

        // Acquire a lock for updating
        let lock_path = metadata.path.with_extension("lock");
        let mut lock = FileLock::new(&lock_path).await;

        if !lock.acquire(10000).await? {
            return Err(ShellBeError::Security(format!(
                "Failed to acquire lock for plugin update: {}", name
            )));
        }

        // Remove the plugin from the repository
        self.repository.remove(name).await?;

        // Backup the plugin directory
        let backup_dir = tempfile::tempdir()
            .map_err(|e| ShellBeError::Io(format!("Failed to create temporary directory: {}", e)))?;
        
        if metadata.path.exists() {
            copy_dir_all(&metadata.path, backup_dir.path())
                .map_err(|e| ShellBeError::Io(format!("Failed to backup plugin directory: {}", e)))?;
            
            fs::remove_dir_all(&metadata.path)
                .map_err(|e| ShellBeError::Io(format!("Failed to remove old plugin directory: {}", e)))?;
        }

        // Install the plugin again
        let result = self.install_from_github(&source_url).await;

        // Release the lock
        lock.release().await?;

        // Restore from backup if installation failed
        if let Err(ref e) = result {
            tracing::error!("Update failed, restoring from backup: {}", e);
            if metadata.path.exists() {
                fs::remove_dir_all(&metadata.path)
                    .map_err(|e| ShellBeError::Io(format!("Failed to clean up failed update: {}", e)))?;
            }
            
            copy_dir_all(backup_dir.path(), &metadata.path)
                .map_err(|e| ShellBeError::Io(format!("Failed to restore plugin from backup: {}", e)))?;

            // Restore metadata
            let restored_metadata = PluginMetadata {
                status: if was_enabled { PluginStatus::Enabled } else { PluginStatus::Disabled },
                updated_at: Some(Utc::now()),
                ..metadata
            };
            self.repository.save(restored_metadata).await?;

            // Re-enable if it was enabled
            if was_enabled {
                self.enable_plugin(name).await?;
            }

            return Err(e.clone());
        }

        // Re-enable if it was enabled
        if was_enabled {
            self.enable_plugin(name).await?;
        }

        tracing::info!("Plugin '{}' updated successfully", name);

        // Return the updated metadata
        match self.repository.get(name).await? {
            Some(updated_metadata) => Ok(updated_metadata),
            None => Err(ShellBeError::NotFound(format!("Plugin not found after update: {}", name))),
        }
    }

    /// Execute a plugin command
    pub async fn execute_command(&self, plugin_name: &str, command: &str, args: &[String]) -> Result<()> {
        // Get the plugin
        let plugin = self.get_loaded_plugin(plugin_name).await?;

        // Check if the command exists
        let commands = plugin.commands();
        if !commands.iter().any(|c| c.name == command) {
            return Err(ShellBeError::Plugin(format!(
                "Command '{}' not found in plugin '{}'", command, plugin_name
            )));
        }

        // Execute the command
        plugin.execute_command(command, args).await
            .map_err(|e| ShellBeError::Plugin(format!("Command execution failed: {}", e)))
    }

    /// Get all loaded plugins
    pub async fn get_loaded_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        let plugins = self.loaded_plugins.read().await;
        plugins.iter().map(|(_, plugin, _)| plugin.clone()).collect()
    }

    /// Execute a hook on all enabled plugins
    pub async fn execute_hook(&self, hook: Hook, profile: Option<&Profile>) -> Result<()> {
        let plugins = self.get_loaded_plugins().await;

        for plugin in plugins {
            if let Err(e) = plugin.execute_hook(hook, profile).await {
                tracing::warn!("Error in plugin hook: {}", e);
            }
        }

        Ok(())
    }

    // Private methods

    /// Load a plugin from a directory
    async fn load_plugin_internal(&self, name: &str, plugin_dir: &Path) -> Result<()> {
        // Check if plugin is already loaded
        {
            let plugins = self.loaded_plugins.read().await;
            if plugins.iter().any(|(n, _, _)| n == name) {
                return Ok(());
            }
        }

        // Find the library file
        let lib_path = find_plugin_library(plugin_dir)
            .with_context(|| format!("Failed to find plugin library in {}", plugin_dir.display()))?;

        // Validate plugin security before loading
        self.security_validator.validate(&lib_path)
            .with_context(|| format!("Plugin security validation failed for {}", lib_path.display()))?;

        // Load the library
        let lib = unsafe { 
            Library::new(&lib_path)
                .map_err(|e| ShellBeError::Plugin(format!("Failed to load plugin library: {}", e)))?
        };

        // Get the create_plugin function
        type CreatePlugin = unsafe fn() -> *mut dyn Plugin;

        let create_plugin: Symbol<CreatePlugin> = unsafe {
            lib.get(b"create_plugin")
                .map_err(|_| ShellBeError::Plugin("Symbol 'create_plugin' not found".to_string()))?
        };

        // Create the plugin
        let plugin = unsafe {
            let raw = create_plugin();
            Arc::from_raw(raw)
        };

        // Verify plugin info
        let info = plugin.info();
        if info.name != name {
            return Err(ShellBeError::Plugin(format!(
                "Plugin name mismatch: expected '{}', got '{}'", name, info.name
            )));
        }

        // Add to loaded plugins
        {
            let mut plugins = self.loaded_plugins.write().await;
            plugins.push((name.to_string(), plugin.clone(), Arc::new(lib)));
        }

        Ok(())
    }

    /// Get a loaded plugin by name
    async fn get_loaded_plugin(&self, name: &str) -> Result<Arc<dyn Plugin>> {
        let plugins = self.loaded_plugins.read().await;
        plugins.iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, plugin, _)| plugin.clone())
            .ok_or_else(|| ShellBeError::NotFound(format!("Plugin not loaded: {}", name)))
    }

    /// Unload a plugin by name
    async fn unload_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.loaded_plugins.write().await;
        let idx = plugins.iter().position(|(n, _, _)| n == name)
            .ok_or_else(|| ShellBeError::NotFound(format!("Plugin not loaded: {}", name)))?;

        // Remove the plugin
        plugins.remove(idx);

        Ok(())
    }
    
    /// Set security validator options
    pub fn set_security_validator(&mut self, validator: PluginSecurityValidator) {
        self.security_validator = validator;
    }
    
    /// Set system requirements
    pub fn set_system_requirements(&mut self, requirements: SystemRequirements) {
        self.system_requirements = requirements;
    }
}

// Helper functions

/// Parse a GitHub URL into owner and repo
fn parse_github_url(url: &str) -> Result<(String, String)> {
    // Extract owner and repo from different GitHub URL formats
    let re = regex::Regex::new(r"github\.com[/:]([^/]+)/([^/]+)")
        .map_err(|_| ShellBeError::Update("Invalid GitHub URL format".to_string()))?;

    if let Some(captures) = re.captures(url) {
        let owner = captures.get(1).unwrap().as_str().to_string();
        let mut repo = captures.get(2).unwrap().as_str().to_string();

        // Remove .git suffix if present
        if repo.ends_with(".git") {
            repo = repo[0..repo.len() - 4].to_string();
        }

        Ok((owner, repo))
    } else {
        Err(ShellBeError::Update(format!("Invalid GitHub URL: {}", url)))
    }
}

/// Find a plugin library file in a directory
fn find_plugin_library(plugin_dir: &Path) -> Result<PathBuf> {
    let lib_extensions = if cfg!(target_os = "windows") {
        vec!["dll"]
    } else if cfg!(target_os = "macos") {
        vec!["dylib"]
    } else {
        vec!["so"]
    };

    for entry in fs::read_dir(plugin_dir)
        .map_err(|e| ShellBeError::Io(format!("Failed to read plugin directory: {}", e)))?
    {
        let entry = entry
            .map_err(|e| ShellBeError::Io(format!("Failed to read directory entry: {}", e)))?;
        
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if lib_extensions.iter().any(|e| ext == *e) {
                    return Ok(path);
                }
            }
        }
    }

    Err(ShellBeError::Plugin(format!("No plugin library found in {}", plugin_dir.display())))
}

/// Copy a directory recursively
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}