use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::error::Error;
use std::path::Path;

/// Current API version
pub const API_VERSION: &str = "2.0.0";

/// Plugin hook types that can be called at various points
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// SSH profile information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Unique name/identifier for the profile
    pub name: String,
    /// Hostname or IP address
    pub hostname: String,
    /// Username for SSH login
    pub username: String,
    /// SSH port, defaults to 22
    pub port: u16,
    /// Path to identity file (private key)
    pub identity_file: Option<String>,
    /// Additional SSH options
    pub options: std::collections::HashMap<String, String>,
}

/// Plugin information
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// API version this plugin is compatible with
    pub api_version: String,
}

/// Plugin command definition for custom commands
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    async fn on_install(&self, plugin_dir: &Path) -> PluginResult {
        Ok(())
    }

    /// Called when the plugin is updated
    async fn on_update(&self, plugin_dir: &Path) -> PluginResult {
        Ok(())
    }
}

/// Macro to declare a plugin factory function
#[macro_export]
macro_rules! declare_plugin {
    ($plugin_type:ty) => {
        #[no_mangle]
        pub extern "C" fn create_plugin() -> *mut dyn shellbe_plugin_sdk::Plugin {
            // Create plugin instance
            let plugin = <$plugin_type>::default();

            // Convert to pointer and leak to prevent Rust from dropping it
            // (will be managed by the host application)
            Box::into_raw(Box::new(plugin))
        }
    };
}

/// Example plugin implementation
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct ExamplePlugin;

    #[async_trait]
    impl Plugin for ExamplePlugin {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "example".to_string(),
                version: "1.0.0".to_string(),
                description: "Example plugin".to_string(),
                author: "ShellBe".to_string(),
                source_url: None,
                api_version: API_VERSION.to_string(),
            }
        }

        fn commands(&self) -> Vec<PluginCommand> {
            vec![
                PluginCommand {
                    name: "hello".to_string(),
                    description: "Say hello".to_string(),
                    usage: "shellbe plugin run example hello [name]".to_string(),
                },
            ]
        }

        async fn execute_hook(&self, hook: Hook, profile: Option<&Profile>) -> PluginResult {
            println!("Hook: {:?}", hook);
            if let Some(profile) = profile {
                println!("Profile: {}", profile.name);
            }
            Ok(())
        }

        async fn execute_command(&self, command: &str, args: &[String]) -> PluginResult {
            match command {
                "hello" => {
                    let name = args.get(0).map(|s| s.as_str()).unwrap_or("world");
                    println!("Hello, {}!", name);
                    Ok(())
                },
                _ => Err(format!("Unknown command: {}", command).into()),
            }
        }
    }

    // Example of how to use the declare_plugin macro
    // declare_plugin!(ExamplePlugin);
}