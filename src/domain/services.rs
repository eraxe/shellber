use crate::domain::models::{Profile, Alias, HistoryEntry};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;

/// ProfileRepository defines the interface for profile storage
#[async_trait]
pub trait ProfileRepository: Send + Sync {
    /// Add a new profile
    async fn add(&self, profile: Profile) -> Result<(), Error>;

    /// Get a profile by name
    async fn get(&self, name: &str) -> Result<Option<Profile>, Error>;

    /// Update an existing profile
    async fn update(&self, profile: Profile) -> Result<(), Error>;

    /// Remove a profile by name
    async fn remove(&self, name: &str) -> Result<(), Error>;

    /// List all profiles
    async fn list(&self) -> Result<Vec<Profile>, Error>;

    /// Check if a profile exists
    async fn exists(&self, name: &str) -> Result<bool, Error>;
}

/// AliasRepository defines the interface for alias storage
#[async_trait]
pub trait AliasRepository: Send + Sync {
    /// Add a new alias
    async fn add(&self, alias: Alias) -> Result<(), Error>;

    /// Get the target profile name for an alias
    async fn get_target(&self, alias_name: &str) -> Result<Option<String>, Error>;

    /// Remove an alias
    async fn remove(&self, alias_name: &str) -> Result<(), Error>;

    /// List all aliases
    async fn list(&self) -> Result<Vec<Alias>, Error>;

    /// List aliases pointing to a specific profile
    async fn list_for_profile(&self, profile_name: &str) -> Result<Vec<Alias>, Error>;
}

/// HistoryRepository defines the interface for connection history storage
#[async_trait]
pub trait HistoryRepository: Send + Sync {
    /// Add a history entry
    async fn add(&self, entry: HistoryEntry) -> Result<(), Error>;

    /// Get recent history entries
    async fn get_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>, Error>;

    /// Get history for a specific profile
    async fn get_for_profile(&self, profile_name: &str) -> Result<Vec<HistoryEntry>, Error>;

    /// Get connection statistics
    async fn get_stats(&self) -> Result<HashMap<String, usize>, Error>;
}

/// SshConfigRepository defines the interface for SSH config file operations
#[async_trait]
pub trait SshConfigRepository: Send + Sync {
    /// Import profiles from SSH config
    async fn import(&self) -> Result<Vec<Profile>, Error>;

    /// Export profiles to SSH config
    async fn export(&self, profiles: &[Profile], replace: bool) -> Result<(), Error>;

    /// Add a single profile to SSH config
    async fn add_profile(&self, profile: &Profile) -> Result<(), Error>;

    /// Remove a profile from SSH config
    async fn remove_profile(&self, profile_name: &str) -> Result<(), Error>;
}

/// SshService defines the interface for SSH operations
#[async_trait]
pub trait SshService: Send + Sync {
    /// Connect to a profile
    async fn connect(&self, profile: &Profile) -> Result<i32, Error>;

    /// Test connection to a profile
    async fn test_connection(&self, profile: &Profile) -> Result<bool, Error>;

    /// Copy SSH key to a remote server
    async fn copy_key(&self, profile: &Profile, key_path: &Path) -> Result<(), Error>;

    /// Generate a new SSH key pair
    async fn generate_key(&self, key_name: &str, comment: Option<&str>) -> Result<(Path, Path), Error>;
}

/// Unified error type for domain services
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Profile already exists: {0}")]
    ProfileAlreadyExists(String),

    #[error("Alias not found: {0}")]
    AliasNotFound(String),

    #[error("Alias already exists: {0}")]
    AliasAlreadyExists(String),

    #[error("SSH error: {0}")]
    SshError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Config error: {0}")]
    ConfigError(String),
}