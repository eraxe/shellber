use crate::domain::{
    Profile, HistoryEntry, ProfileRepository,
    AliasRepository, HistoryRepository, SshService,
    DomainError, EventBus, Event, Hook, Plugin,
};
use std::sync::Arc;
use std::time::Instant;

/// ConnectionService manages SSH connections
pub struct ConnectionService {
    profile_repository: Arc<dyn ProfileRepository>,
    alias_repository: Arc<dyn AliasRepository>,
    history_repository: Arc<dyn HistoryRepository>,
    ssh_service: Arc<dyn SshService>,
    event_bus: Arc<EventBus>,
    plugins: Arc<Vec<Arc<dyn Plugin>>>,
}

impl ConnectionService {
    /// Create a new ConnectionService with the provided dependencies
    pub fn new(
        profile_repository: Arc<dyn ProfileRepository>,
        alias_repository: Arc<dyn AliasRepository>,
        history_repository: Arc<dyn HistoryRepository>,
        ssh_service: Arc<dyn SshService>,
        event_bus: Arc<EventBus>,
        plugins: Arc<Vec<Arc<dyn Plugin>>>,
    ) -> Self {
        Self {
            profile_repository,
            alias_repository,
            history_repository,
            ssh_service,
            event_bus,
            plugins,
        }
    }

    /// Connect to a profile or alias
    pub async fn connect(&self, name: &str) -> Result<i32, DomainError> {
        // First check if this is an alias
        let profile_name = match self.alias_repository.get_target(name).await? {
            Some(target) => target,
            None => name.to_string(),
        };

        // Get the profile
        let mut profile = match self.profile_repository.get(&profile_name).await? {
            Some(profile) => profile,
            None => return Err(DomainError::ProfileNotFound(profile_name)),
        };

        // Create a history entry
        let mut entry = HistoryEntry::new(&profile.name, &profile.hostname);

        // Publish connection started event
        self.event_bus.publish(Event::ConnectionStarted(profile.clone()));

        // Run pre-connect plugin hooks
        for plugin in self.plugins.iter() {
            if let Err(e) = plugin.execute_hook(Hook::PreConnect, Some(&profile)).await {
                tracing::warn!("Plugin error in pre-connect hook: {}", e);
            }
        }

        // Connect and measure time
        let start = Instant::now();
        let exit_code = self.ssh_service.connect(&profile).await?;
        let duration = start.elapsed();

        // Update history entry with result
        entry = entry.with_result(exit_code, duration);

        // Update profile last used time
        profile.mark_as_used();
        self.profile_repository.update(profile.clone()).await?;

        // Save history
        self.history_repository.add(entry.clone()).await?;

        // Run post-connect plugin hooks
        for plugin in self.plugins.iter() {
            if let Err(e) = plugin.execute_hook(Hook::PostDisconnect, Some(&profile)).await {
                tracing::warn!("Plugin error in post-disconnect hook: {}", e);
            }
        }

        // Publish connection ended event
        self.event_bus.publish(Event::ConnectionEnded(entry));

        Ok(exit_code)
    }

    /// Test connection to a profile or alias
    pub async fn test_connection(&self, name: &str) -> Result<bool, DomainError> {
        // First check if this is an alias
        let profile_name = match self.alias_repository.get_target(name).await? {
            Some(target) => target,
            None => name.to_string(),
        };

        // Get the profile
        let profile = match self.profile_repository.get(&profile_name).await? {
            Some(profile) => profile,
            None => return Err(DomainError::ProfileNotFound(profile_name)),
        };

        // Test the connection
        let result = self.ssh_service.test_connection(&profile).await?;

        // Run appropriate plugin hooks based on result
        let hook = if result {
            Hook::TestSuccess
        } else {
            Hook::TestFailure
        };

        for plugin in self.plugins.iter() {
            if let Err(e) = plugin.execute_hook(hook, Some(&profile)).await {
                tracing::warn!("Plugin error in test hook: {}", e);
            }
        }

        Ok(result)
    }

    /// Copy SSH key to a remote server
    pub async fn copy_ssh_key(&self, name: &str, key_path: &std::path::Path) -> Result<(), DomainError> {
        // First check if this is an alias
        let profile_name = match self.alias_repository.get_target(name).await? {
            Some(target) => target,
            None => name.to_string(),
        };

        // Get the profile
        let profile = match self.profile_repository.get(&profile_name).await? {
            Some(profile) => profile,
            None => return Err(DomainError::ProfileNotFound(profile_name)),
        };

        // Copy the key
        self.ssh_service.copy_key(&profile, key_path).await
    }

    /// Get recent connection history
    pub async fn get_recent_history(&self, limit: usize) -> Result<Vec<HistoryEntry>, DomainError> {
        self.history_repository.get_recent(limit).await
    }

    /// Get connection history for a specific profile
    pub async fn get_profile_history(&self, profile_name: &str) -> Result<Vec<HistoryEntry>, DomainError> {
        // Check if profile exists
        if !self.profile_repository.exists(profile_name).await? {
            return Err(DomainError::ProfileNotFound(profile_name.to_string()));
        }

        self.history_repository.get_for_profile(profile_name).await
    }

    /// Get connection statistics
    pub async fn get_connection_stats(&self) -> Result<Vec<(String, usize)>, DomainError> {
        let stats = self.history_repository.get_stats().await?;

        // Convert HashMap to Vec of tuples
        let mut stats_vec: Vec<(String, usize)> = stats.into_iter().collect();

        // Sort by count in descending order
        stats_vec.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(stats_vec)
    }
}