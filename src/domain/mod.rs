pub mod models;
pub mod events;
pub mod plugin;
pub mod services;

// Re-export common types
pub use models::{Profile, Alias, HistoryEntry, ConnectionStats};
pub use events::{Event, EventBus, EventListener};
pub use plugin::{Plugin, PluginInfo, PluginCommand, Hook, PluginStatus, PluginMetadata};
pub use services::{
    ProfileRepository, AliasRepository, HistoryRepository,
    SshConfigRepository, SshService, Error as DomainError
};