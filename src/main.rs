pub mod domain;
pub mod application;
pub mod infrastructure;
pub mod interface;

/// Re-export common types
pub use domain::{
    Profile, Alias, HistoryEntry, ConnectionStats,
    Event, EventBus, EventListener,
    Plugin, PluginInfo, PluginCommand, Hook, PluginStatus, PluginMetadata,
    DomainError,
};

pub use application::{
    ProfileService, ConnectionService, AliasService,
    PluginService, SshConfigService, PluginError,
};

pub use infrastructure::{
    FileProfileRepository, FileAliasRepository, FileHistoryRepository,
    FilePluginRepository, FileSshConfigRepository, ThrushSshService,
};

pub use interface::{Cli, CommandHandler};