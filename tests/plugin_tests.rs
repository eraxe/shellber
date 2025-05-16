use shellbe::{
    Plugin, PluginInfo, PluginStatus, PluginMetadata,
    Hook, Profile,
    application::PluginService,
    infrastructure::FilePluginRepository,
    domain::EventBus,
};
use std::sync::Arc;
use std::path::PathBuf;
use tempfile::TempDir;
use async_trait::async_trait;

#[tokio::test]
async fn test_plugin_repository() {
    // Create a temporary directory for test
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().to_path_buf();

    // Create plugin repository
    let plugin_repo = FilePluginRepository::new(config_dir.clone(), "plugins.json".to_string()).await.unwrap();

    // Create some test metadata
    let plugin_path = config_dir.join("plugins").join("test-plugin");
    std::fs::create_dir_all(&plugin_path).unwrap();

    let metadata = PluginMetadata {
        info: PluginInfo {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            source_url: None,
        },
        status: PluginStatus::Enabled,
        path: plugin_path,
        installed_at: chrono::Utc::now(),
        updated_at: None,
    };

    // Save metadata
    plugin_repo.save(metadata.clone()).await.unwrap();

    // Retrieve metadata
    let retrieved = plugin_repo.get("test-plugin").await.unwrap().unwrap();
    assert_eq!(retrieved.info.name, "test-plugin");
    assert_eq!(retrieved.info.version, "1.0.0");
    assert_eq!(retrieved.status, PluginStatus::Enabled);

    // Update status
    plugin_repo.update_status("test-plugin", PluginStatus::Disabled).await.unwrap();

    // Verify updated status
    let updated = plugin_repo.get("test-plugin").await.unwrap().unwrap();
    assert_eq!(updated.status, PluginStatus::Disabled);

    // List plugins
    let plugins = plugin_repo.list().await.unwrap();
    assert_eq!(plugins.len(), 1);

    // Remove plugin
    plugin_repo.remove("test-plugin").await.unwrap();

    // Verify removal
    let removed = plugin_repo.get("test-plugin").await.unwrap();
    assert!(removed.is_none());
}

// Mock plugin for testing
struct MockPlugin;

#[async_trait]
impl Plugin for MockPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "mock-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Mock plugin for testing".to_string(),
            author: "Test Author".to_string(),
            source_url: None,
        }
    }

    fn commands(&self) -> Vec<shellbe::PluginCommand> {
        vec![
            shellbe::PluginCommand {
                name: "test".to_string(),
                description: "Test command".to_string(),
                usage: "test".to_string(),
            }
        ]
    }

    async fn execute_hook(&self, _hook: Hook, _profile: Option<&Profile>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Just return success for testing
        Ok(())
    }

    async fn execute_command(&self, _command: &str, _args: &[String]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Just return success for testing
        Ok(())
    }
}

// This test won't load an actual plugin but tests the repository interactions
#[tokio::test]
async fn test_plugin_service() {
    // Create a temporary directory for test
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().to_path_buf();
    let plugins_dir = config_dir.join("plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();

    // Create plugin repository
    let plugin_repo = Arc::new(
        FilePluginRepository::new(config_dir.clone(), "plugins.json".to_string()).await.unwrap()
    );

    // Create event bus
    let event_bus = Arc::new(EventBus::new());

    // Create plugin service
    let plugin_service = PluginService::new(
        plugin_repo.clone(),
        event_bus,
        plugins_dir.clone(),
    );

    // Initialize the service
    plugin_service.initialize().await.unwrap();

    // Test listing plugins
    let plugins = plugin_service.list_plugins().await.unwrap();
    assert_eq!(plugins.len(), 0); // Empty at first

    // Create some test metadata
    let plugin_path = plugins_dir.join("test-plugin");
    std::fs::create_dir_all(&plugin_path).unwrap();

    let metadata = PluginMetadata {
        info: PluginInfo {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            source_url: None,
        },
        status: PluginStatus::Disabled, // Start disabled
        path: plugin_path,
        installed_at: chrono::Utc::now(),
        updated_at: None,
    };

    // Save metadata
    plugin_repo.save(metadata.clone()).await.unwrap();

    // Test getting plugin
    let plugin = plugin_service.get_plugin("test-plugin").await.unwrap();
    assert_eq!(plugin.info.name, "test-plugin");
    assert_eq!(plugin.status, PluginStatus::Disabled);

    // Test update status
    plugin_repo.update_status("test-plugin", PluginStatus::Enabled).await.unwrap();
    let updated = plugin_service.get_plugin("test-plugin").await.unwrap();
    assert_eq!(updated.status, PluginStatus::Enabled);
}