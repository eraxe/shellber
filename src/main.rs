use clap::Parser;
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
    utils::{SystemRequirements, PluginSecurityValidator},
    ShellBeError, Result, ErrorContext,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize error handling and tracing
    color_eyre::install()
        .map_err(|e| ShellBeError::Config(format!("Failed to initialize error handling: {}", e)))?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Check system requirements
    let system_requirements = SystemRequirements::default();
    system_requirements.all_requirements_met()
        .with_context(|| "Failed to start: system requirements not met".to_string())?;

    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize config directory
    let config_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".shellbe");

    // Create directory if it doesn't exist
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| ShellBeError::Io(format!("Failed to create config directory: {}", e)))?;

        // Set proper permissions on Unix platforms
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&config_dir)
                .map_err(|e| ShellBeError::Io(format!("Failed to get directory metadata: {}", e)))?;

            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);

            std::fs::set_permissions(&config_dir, permissions)
                .map_err(|e| ShellBeError::Io(format!("Failed to set directory permissions: {}", e)))?;
        }
    }

    // Initialize event bus
    let event_bus = Arc::new(EventBus::new());

    // Initialize repositories
    let storage_config = FileStorageConfig {
        config_dir: config_dir.clone(),
        profiles_file: "profiles.json".to_string(),
    };

    let profile_repository = Arc::new(FileProfileRepository::new(storage_config).await
        .map_err(|e| ShellBeError::Config(format!("Failed to initialize profile repository: {}", e)))?);

    let alias_repository = Arc::new(FileAliasRepository::new(config_dir.clone(), "aliases.json".to_string()).await
        .map_err(|e| ShellBeError::Config(format!("Failed to initialize alias repository: {}", e)))?);

    let history_repository = Arc::new(FileHistoryRepository::new(config_dir.clone(), "history.json".to_string()).await
        .map_err(|e| ShellBeError::Config(format!("Failed to initialize history repository: {}", e)))?);

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
        std::fs::create_dir_all(&plugins_dir)
            .map_err(|e| ShellBeError::Io(format!("Failed to create plugins directory: {}", e)))?;
    }

    let plugin_repository = Arc::new(FilePluginRepository::new(config_dir.clone(), "plugins.json".to_string()).await
        .map_err(|e| ShellBeError::Config(format!("Failed to initialize plugin repository: {}", e)))?);

    // Create plugin service with security validation
    let mut plugin_service = PluginService::new(
        plugin_repository,
        event_bus.clone(),
        plugins_dir.clone(),
    );

    // Set security validator options - adjust as needed for your security requirements
    let plugin_security = PluginSecurityValidator::default();
    plugin_service.set_security_validator(plugin_security);

    // Set system requirements for plugins
    plugin_service.set_system_requirements(system_requirements);

    // Create the Arc for plugin service
    let plugin_service = Arc::new(plugin_service);

    // Initialize the plugin system
    plugin_service.initialize().await
        .map_err(|e| ShellBeError::Plugin(format!("Failed to initialize plugin system: {}", e)))?;

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
        match command_handler.handle_command(command).await {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Command error: {}", e);
                return Err(ShellBeError::Config(format!("Failed to execute command: {}", e)));
            }
        }
    } else {
        // Print help if no command provided
        println!("No command provided. Use `shellbe help` to see available commands.");
        if let Err(e) = cli.into_app().print_help() {
            tracing::error!("Failed to print help: {}", e);
        }
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