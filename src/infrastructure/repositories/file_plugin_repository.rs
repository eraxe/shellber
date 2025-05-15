use crate::domain::{PluginMetadata, PluginStatus, PluginInfo};
use crate::application::PluginError;
use async_trait::async_trait;
use std::path::PathBuf;
use std::fs;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

/// Plugin repository trait for the application layer
#[async_trait]
pub trait PluginRepository: Send + Sync {
    /// Get plugin metadata by name
    async fn get(&self, name: &str) -> Result<Option<PluginMetadata>, PluginError>;

    /// List all plugins
    async fn list(&self) -> Result<Vec<PluginMetadata>, PluginError>;

    /// Save plugin metadata
    async fn save(&self, metadata: PluginMetadata) -> Result<(), PluginError>;

    /// Remove plugin metadata
    async fn remove(&self, name: &str) -> Result<(), PluginError>;

    /// Update plugin status
    async fn update_status(&self, name: &str, status: PluginStatus) -> Result<(), PluginError>;
}

/// Serializable plugin metadata for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializablePluginMetadata {
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
    /// Plugin author
    pub author: String,
    /// Source URL
    pub source_url: Option<String>,
    /// Plugin status
    pub status: PluginStatus,
    /// Plugin path
    pub path: String,
    /// Installation date
    pub installed_at: chrono::DateTime<chrono::Utc>,
    /// Last update date
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<PluginMetadata> for SerializablePluginMetadata {
    fn from(metadata: PluginMetadata) -> Self {
        Self {
            name: metadata.info.name,
            version: metadata.info.version,
            description: metadata.info.description,
            author: metadata.info.author,
            source_url: metadata.info.source_url,
            status: metadata.status,
            path: metadata.path.to_string_lossy().to_string(),
            installed_at: metadata.installed_at,
            updated_at: metadata.updated_at,
        }
    }
}

impl From<SerializablePluginMetadata> for PluginMetadata {
    fn from(serializable: SerializablePluginMetadata) -> Self {
        Self {
            info: PluginInfo {
                name: serializable.name,
                version: serializable.version,
                description: serializable.description,
                author: serializable.author,
                source_url: serializable.source_url,
            },
            status: serializable.status,
            path: PathBuf::from(serializable.path),
            installed_at: serializable.installed_at,
            updated_at: serializable.updated_at,
        }
    }
}

/// File-based implementation of the plugin repository
pub struct FilePluginRepository {
    config_dir: PathBuf,
    plugins_file: String,
    plugins: Arc<RwLock<Vec<SerializablePluginMetadata>>>,
}

impl FilePluginRepository {
    /// Create a new file-based plugin repository
    pub async fn new(config_dir: PathBuf, plugins_file: String) -> Result<Self, PluginError> {
        // Create config directory if it doesn't exist
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)
                .map_err(|e| PluginError::IoError(e))?;
        }

        let plugins_path = config_dir.join(&plugins_file);
        let plugins: Vec<SerializablePluginMetadata> = if plugins_path.exists() {
            let file = fs::File::open(&plugins_path)
                .map_err(|e| PluginError::IoError(e))?;

            serde_json::from_reader(file)
                .map_err(|e| PluginError::InstallationFailed(format!("Failed to parse plugins: {}", e)))?
        } else {
            Vec::new()
        };

        Ok(Self {
            config_dir,
            plugins_file,
            plugins: Arc::new(RwLock::new(plugins)),
        })
    }

    /// Save plugins to disk
    async fn save_plugins(&self) -> Result<(), PluginError> {
        let plugins_path = self.config_dir.join(&self.plugins_file);
        let plugins = self.plugins.read().await;

        let file = fs::File::create(&plugins_path)
            .map_err(|e| PluginError::IoError(e))?;

        serde_json::to_writer_pretty(file, &*plugins)
            .map_err(|e| PluginError::InstallationFailed(format!("Failed to save plugins: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl PluginRepository for FilePluginRepository {
    /// Get plugin metadata by name
    async fn get(&self, name: &str) -> Result<Option<PluginMetadata>, PluginError> {
        let plugins = self.plugins.read().await;

        let plugin = plugins.iter()
            .find(|p| p.name == name)
            .cloned()
            .map(Into::into);

        Ok(plugin)
    }

    /// List all plugins
    async fn list(&self) -> Result<Vec<PluginMetadata>, PluginError> {
        let plugins = self.plugins.read().await;

        let result = plugins.iter()
            .cloned()
            .map(Into::into)
            .collect();

        Ok(result)
    }

    /// Save plugin metadata
    async fn save(&self, metadata: PluginMetadata) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;

        // Remove existing plugin with the same name
        plugins.retain(|p| p.name != metadata.info.name);

        // Add the new metadata
        plugins.push(metadata.into());

        drop(plugins);

        self.save_plugins().await
    }

    /// Remove plugin metadata
    async fn remove(&self, name: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;

        let initial_len = plugins.len();
        plugins.retain(|p| p.name != name);

        if plugins.len() == initial_len {
            return Err(PluginError::NotFound(name.to_string()));
        }

        drop(plugins);

        self.save_plugins().await
    }

    /// Update plugin status
    async fn update_status(&self, name: &str, status: PluginStatus) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;

        let plugin = plugins.iter_mut()
            .find(|p| p.name == name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        plugin.status = status;

        drop(plugins);

        self.save_plugins().await
    }
}