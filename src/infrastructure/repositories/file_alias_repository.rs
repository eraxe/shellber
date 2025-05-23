use crate::domain::{AliasRepository, Alias, DomainError};
use crate::utils::{FileLock, ensure_directory, ensure_file};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use std::fs;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// File-based implementation of the alias repository
pub struct FileAliasRepository {
    config_dir: PathBuf,
    aliases_file: String,
    aliases: Arc<RwLock<HashMap<String, String>>>,
}

impl FileAliasRepository {
    /// Create a new file-based alias repository
    pub async fn new(config_dir: PathBuf, aliases_file: String) -> Result<Self, DomainError> {
        // Create config directory if it doesn't exist
        ensure_directory(&config_dir).await
            .map_err(|e| DomainError::IoError(e))?;

        let aliases_path = config_dir.join(&aliases_file);
        let aliases: HashMap<String, String> = if aliases_path.exists() {
            let file = fs::File::open(&aliases_path)
                .map_err(|e| DomainError::IoError(e))?;

            serde_json::from_reader(file)
                .map_err(|e| DomainError::ConfigError(format!("Failed to parse aliases: {}", e)))?
        } else {
            // Create empty aliases file
            ensure_file(&aliases_path, Some("{}")).await
                .map_err(|e| DomainError::IoError(e))?;
            HashMap::new()
        };

        Ok(Self {
            config_dir,
            aliases_file,
            aliases: Arc::new(RwLock::new(aliases)),
        })
    }

    /// Save aliases to disk with proper file locking
    async fn save_aliases(&self) -> Result<(), DomainError> {
        let aliases_path = self.config_dir.join(&self.aliases_file);

        // Acquire a lock for writing
        let mut lock = FileLock::new(&aliases_path).await;
        if !lock.acquire(5000).await.map_err(|e| DomainError::IoError(e))? {
            return Err(DomainError::ConfigError("Failed to acquire lock for writing aliases".to_string()));
        }

        // Get a snapshot of the aliases
        let aliases = {
            let aliases = self.aliases.read().await;
            aliases.clone()
        };

        // Write to a temporary file first
        let temp_path = aliases_path.with_extension("temp");
        let file = fs::File::create(&temp_path)
            .map_err(|e| DomainError::IoError(e))?;

        serde_json::to_writer_pretty(file, &aliases)
            .map_err(|e| DomainError::ConfigError(format!("Failed to save aliases: {}", e)))?;

        // Rename the temporary file to the actual file
        // This provides atomic file replacement
        fs::rename(&temp_path, &aliases_path)
            .map_err(|e| DomainError::IoError(e))?;

        // Release the lock
        lock.release().await.map_err(|e| DomainError::IoError(e))?;

        Ok(())
    }
}

#[async_trait]
impl AliasRepository for FileAliasRepository {
    /// Add a new alias
    async fn add(&self, alias: Alias) -> Result<(), DomainError> {
        let mut aliases = self.aliases.write().await;

        if aliases.contains_key(&alias.name) {
            return Err(DomainError::AliasAlreadyExists(alias.name));
        }

        aliases.insert(alias.name, alias.target);
        drop(aliases);

        self.save_aliases().await
    }

    /// Get the target profile name for an alias
    async fn get_target(&self, alias_name: &str) -> Result<Option<String>, DomainError> {
        let aliases = self.aliases.read().await;
        Ok(aliases.get(alias_name).cloned())
    }

    /// Remove an alias
    async fn remove(&self, alias_name: &str) -> Result<(), DomainError> {
        let mut aliases = self.aliases.write().await;

        if !aliases.contains_key(alias_name) {
            return Err(DomainError::AliasNotFound(alias_name.to_string()));
        }

        aliases.remove(alias_name);
        drop(aliases);

        self.save_aliases().await
    }

    /// List all aliases
    async fn list(&self) -> Result<Vec<Alias>, DomainError> {
        let aliases = self.aliases.read().await;
        let result = aliases.iter()
            .map(|(name, target)| Alias::new(name, target))
            .collect();

        Ok(result)
    }

    /// List aliases pointing to a specific profile
    async fn list_for_profile(&self, profile_name: &str) -> Result<Vec<Alias>, DomainError> {
        let aliases = self.aliases.read().await;
        let result = aliases.iter()
            .filter(|(_, target)| *target == profile_name)
            .map(|(name, target)| Alias::new(name, target))
            .collect();

        Ok(result)
    }
}