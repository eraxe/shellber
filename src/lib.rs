pub mod domain;
pub mod application;
pub mod infrastructure;
pub mod interface;
pub mod utils;
pub mod errors;

/// Re-export common types
pub use domain::{
    Profile, Alias, HistoryEntry, ConnectionStats,
    Event, EventBus, EventListener,
    Plugin, PluginInfo, PluginCommand, Hook, PluginStatus, PluginMetadata,
};

pub use application::{
    ProfileService, ConnectionService, AliasService,
    PluginService, SshConfigService, UpdateService,
};

pub use infrastructure::{
    FileProfileRepository, FileAliasRepository, FileHistoryRepository,
    FilePluginRepository, FileSshConfigRepository, ThrushSshService,
};

pub use interface::{Cli, CommandHandler};

// Re-export error and result types
pub use errors::{ShellBeError, Result, ErrorContext};

// Re-export useful utility functions
pub use utils::{
    ensure_directory, ensure_file, backup_file,
    shellbe_config_dir, ssh_config_dir,
    FileLock, PluginSecurityValidator, SystemRequirements
};