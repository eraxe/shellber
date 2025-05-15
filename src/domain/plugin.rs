use crate::domain::models::Profile;
use async_trait::async_trait;
use std::error::Error;
use std::path::Path;

/// Plugin hook types that can be called at various points
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    /// Before establishing an SSH connection
    PreConnect,
    /// After an SSH connection has been established
    PostConnect,
    /// After an SSH connection has been closed
    PostDisconnect,
    /// When a connection test succeeds
    TestSuccess,
    /// When a connection test fails
    TestFailure,
    /// When profile information is displayed
    ProfileInfo,
    /// When a plugin is enabled
    PluginEnabled,
    /// When a plugin is disabled
    PluginDisabled,
}

/// Plugin information
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Unique name of the plugin
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
    /// Plugin author
    pub author: String,
    /// Source URL (e.g., GitHub repository)
    pub source_url: Option<String>,
}

/// Plugin command definition for custom commands
#[derive(Debug, Clone)]
pub struct PluginCommand {
    /// Command name
    pub name: String,
    /// Command description
    pub description: String,
    /// Command usage example
    pub usage: String,
}

/// Result type for plugin operations
pub type PluginResult = Result<(), Box<dyn Error + Send + Sync>>;

/// Plugin trait defining the interface for all plugins
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin information
    fn info(&self) -> PluginInfo;

    /// Get available plugin commands
    fn commands(&self) -> Vec<PluginCommand>;

    /// Execute a plugin hook
    async fn execute_hook(&self, hook: Hook, profile: Option<&Profile>) -> PluginResult;

    /// Execute a plugin command
    async fn execute_command(&self, command: &str, args: &[String]) -> PluginResult;

    /// Called when the plugin is enabled
    async fn on_enable(&self) -> PluginResult {
        Ok(())
    }

    /// Called when the plugin is disabled
    async fn on_disable(&self) -> PluginResult {
        Ok(())
    }

    /// Called when the plugin is first installed
    async fn on_install(&self, _plugin_dir: &Path) -> PluginResult {
        Ok(())
    }

    /// Called when the plugin is updated
    async fn on_update(&self, _plugin_dir: &Path) -> PluginResult {
        Ok(())
    }
}

/// Plugin status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    /// Plugin is enabled and active
    Enabled,
    /// Plugin is installed but disabled
    Disabled,
}

/// Metadata for an installed plugin
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Plugin information
    pub info: PluginInfo,
    /// Plugin status
    pub status: PluginStatus,
    /// Plugin installation path
    pub path: std::path::PathBuf,
    /// Installation date
    pub installed_at: chrono::DateTime<chrono::Utc>,
    /// Last update date
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}