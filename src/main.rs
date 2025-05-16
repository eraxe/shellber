use clap::Parser;
use color_eyre::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use shellbe::{
    application::{
        AliasService, ConnectionService, ProfileService, PluginService, SshConfigService,
    },
    domain::EventBus,
    infrastructure::{
        FileAliasRepository, FileHistoryRepository, FilePluginRepository,
        FileProfileRepository, FileSshConfigRepository, ThrushSshService,
    },
    interface::{Cli, CommandHandler},
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize error handling and tracing
    color_eyre::install()?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize config directory
    let config_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".shellbe");

    // Create directory if it doesn't exist
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
        // Set proper permissions on Unix platforms
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&config_dir)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(&config_dir, permissions)?;
        }
    }

    // Initialize event bus
    let event_bus = Arc::new(EventBus::new());

    // Initialize repositories
    let storage_config = FileStorageConfig {
        config_dir: config_dir.clone(),
        profiles_file: "profiles.json".to_string(),
    };

    let profile_repository = Arc::new(FileProfileRepository::new(storage_config).await?);
    let alias_repository = Arc::new(FileAliasRepository::new(config_dir.clone(), "aliases.json".to_string()).await?);
    let history_repository = Arc::new(FileHistoryRepository::new(config_dir.clone(), "history.json".to_string()).await?);

    // Initialize SSH service
    let ssh_service = Arc::new(ThrushSshService::new());

    // Initialize SSH config repository
    let ssh_config_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("config");
    let ssh_config_repository = Arc::new(FileSshConfigRepository::new(ssh_config_path));

    // Initialize plugin system
    let plugins_dir = config_dir.join("plugins");
    if !plugins_dir.exists() {
        std::fs::create_dir_all(&plugins_dir)?;
    }
    let plugin_repository = Arc::new(FilePluginRepository::new(config_dir.clone(), "plugins.json".to_string()).await?);
    let plugin_service = Arc::new(PluginService::new(
        plugin_repository,
        event_bus.clone(),
        plugins_dir.clone(),
    ));

    // Initialize the plugin system
    plugin_service.initialize().await?;

    // Initialize services
    let profile_service = Arc::new(ProfileService::new(profile_repository.clone(), event_bus.clone()));
    let alias_service = Arc::new(AliasService::new(alias_repository, profile_repository.clone()));
    let connection_service = Arc::new(ConnectionService::new(
        profile_repository,
        alias_service.clone(),
        history_repository,
        ssh_service,
        event_bus.clone(),
        Arc::new(plugin_service.get_loaded_plugins().await),
    ));
    let ssh_config_service = Arc::new(SshConfigService::new(ssh_config_repository));

    // Create command handler
    let command_handler = CommandHandler::new(
        profile_service,
        connection_service,
        alias_service,
        plugin_service,
        ssh_config_service,
    );

    // Handle command
    if let Some(command) = cli.command {
        command_handler.handle_command(command).await?;
    } else {
        // Print help if no command provided
        println!("No command provided. Use `shellbe help` to see available commands.");
        cli.into_app().print_help()?;
    }

    Ok(())
}

// Helper struct for FileProfileRepository configuration
// This should be moved to the appropriate module in a full refactoring
#[derive(Debug, Clone)]
pub struct FileStorageConfig {
    /// Directory where configuration files are stored
    pub config_dir: PathBuf,
    /// Profile configuration file name
    pub profiles_file: String,
}