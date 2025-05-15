use crate::domain::{ProfileRepository, Profile, DomainError};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{Read, Write};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Struct for configuring the file storage
#[derive(Debug, Clone)]
pub struct FileStorageConfig {
    /// Directory where configuration files are stored
    pub config_dir: PathBuf,
    /// Profile configuration file name
    pub profiles_file: String,
}

impl Default for FileStorageConfig {
    fn default() -> Self {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".shellbe");

        Self {
            config_dir,
            profiles_file: "profiles.json".to_string(),
        }
    }
}

/// File-based implementation of the profile repository
pub struct FileProfileRepository {
    config: FileStorageConfig,
    profiles: Arc<RwLock<HashMap<String, Profile>>>,
}

impl FileProfileRepository {
    /// Create a new file-based profile repository
    pub async fn new(config: FileStorageConfig) -> Result<Self, DomainError> {
        // Create config directory if it doesn't exist
        if !config.config_dir.exists() {
            fs::create_dir_all(&config.config_dir)
                .map_err(|e| DomainError::IoError(e))?;
        }

        let profiles_path = config.config_dir.join(&config.profiles_file);
        let profiles = if profiles_path.exists() {
            let file = fs::File::open(&profiles_path)
                .map_err(|e| DomainError::IoError(e))?;

            serde_json::from_reader(file)
                .map_err(|e| DomainError::ConfigError(format!("Failed to parse profiles: {}", e)))?
        } else {
            HashMap::new()
        };

        Ok(Self {
            config,
            profiles: Arc::new(RwLock::new(profiles)),
        })
    }

    /// Save profiles to disk
    async fn save_profiles(&self) -> Result<(), DomainError> {
        let profiles_path = self.config.config_dir.join(&self.config.profiles_file);
        let profiles = self.profiles.read().await;

        let file = fs::File::create(&profiles_path)
            .map_err(|e| DomainError::IoError(e))?;

        serde_json::to_writer_pretty(file, &*profiles)
            .map_err(|e| DomainError::ConfigError(format!("Failed to save profiles: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl ProfileRepository for FileProfileRepository {
    /// Add a new profile
    async fn add(&self, profile: Profile) -> Result<(), DomainError> {
        let mut profiles = self.profiles.write().await;

        if profiles.contains_key(&profile.name) {
            return Err(DomainError::ProfileAlreadyExists(profile.name));
        }

        profiles.insert(profile.name.clone(), profile);
        drop(profiles);

        self.save_profiles().await
    }

    /// Get a profile by name
    async fn get(&self, name: &str) -> Result<Option<Profile>, DomainError> {
        let profiles = self.profiles.read().await;
        Ok(profiles.get(name).cloned())
    }

    /// Update an existing profile
    async fn update(&self, profile: Profile) -> Result<(), DomainError> {
        let mut profiles = self.profiles.write().await;

        if !profiles.contains_key(&profile.name) {
            return Err(DomainError::ProfileNotFound(profile.name));
        }

        profiles.insert(profile.name.clone(), profile);
        drop(profiles);

        self.save_profiles().await
    }

    /// Remove a profile by name
    async fn remove(&self, name: &str) -> Result<(), DomainError> {
        let mut profiles = self.profiles.write().await;

        if !profiles.contains_key(name) {
            return Err(DomainError::ProfileNotFound(name.to_string()));
        }

        profiles.remove(name);
        drop(profiles);

        self.save_profiles().await
    }

    /// List all profiles
    async fn list(&self) -> Result<Vec<Profile>, DomainError> {
        let profiles = self.profiles.read().await;
        Ok(profiles.values().cloned().collect())
    }

    /// Check if a profile exists
    async fn exists(&self, name: &str) -> Result<bool, DomainError> {
        let profiles = self.profiles.read().await;
        Ok(profiles.contains_key(name))
    }
}