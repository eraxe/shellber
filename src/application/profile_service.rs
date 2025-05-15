use crate::domain::{
    Profile, ProfileRepository, Event, EventBus,
    DomainError,
};
use std::sync::Arc;

/// ProfileService manages SSH profiles
pub struct ProfileService {
    repository: Arc<dyn ProfileRepository>,
    event_bus: Arc<EventBus>,
}

impl ProfileService {
    /// Create a new ProfileService with the provided repository and event bus
    pub fn new(repository: Arc<dyn ProfileRepository>, event_bus: Arc<EventBus>) -> Self {
        Self {
            repository,
            event_bus,
        }
    }

    /// Add a new profile
    pub async fn add_profile(&self, profile: Profile) -> Result<(), DomainError> {
        // Check if profile already exists
        if self.repository.exists(&profile.name).await? {
            return Err(DomainError::ProfileAlreadyExists(profile.name));
        }

        // Add the profile
        self.repository.add(profile.clone()).await?;

        // Publish event
        self.event_bus.publish(Event::ProfileCreated(profile));

        Ok(())
    }

    /// Get a profile by name
    pub async fn get_profile(&self, name: &str) -> Result<Profile, DomainError> {
        match self.repository.get(name).await? {
            Some(profile) => Ok(profile),
            None => Err(DomainError::ProfileNotFound(name.to_string())),
        }
    }

    /// Update an existing profile
    pub async fn update_profile(&self, profile: Profile) -> Result<(), DomainError> {
        // Check if profile exists
        if !self.repository.exists(&profile.name).await? {
            return Err(DomainError::ProfileNotFound(profile.name.clone()));
        }

        // Update the profile with current timestamp
        let mut updated_profile = profile.clone();
        updated_profile.mark_as_updated();

        // Update the profile
        self.repository.update(updated_profile.clone()).await?;

        // Publish event
        self.event_bus.publish(Event::ProfileUpdated(updated_profile));

        Ok(())
    }

    /// Remove a profile by name
    pub async fn remove_profile(&self, name: &str) -> Result<(), DomainError> {
        // Check if profile exists
        if !self.repository.exists(name).await? {
            return Err(DomainError::ProfileNotFound(name.to_string()));
        }

        // Remove the profile
        self.repository.remove(name).await?;

        // Publish event
        self.event_bus.publish(Event::ProfileRemoved(name.to_string()));

        Ok(())
    }

    /// List all profiles
    pub async fn list_profiles(&self) -> Result<Vec<Profile>, DomainError> {
        self.repository.list().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::events::tests::TestEventListener;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct MockProfileRepository {
        profiles: Mutex<HashMap<String, Profile>>,
    }

    impl MockProfileRepository {
        fn new() -> Self {
            Self {
                profiles: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl ProfileRepository for MockProfileRepository {
        async fn add(&self, profile: Profile) -> Result<(), DomainError> {
            let mut profiles = self.profiles.lock().unwrap();
            profiles.insert(profile.name.clone(), profile);
            Ok(())
        }

        async fn get(&self, name: &str) -> Result<Option<Profile>, DomainError> {
            let profiles = self.profiles.lock().unwrap();
            Ok(profiles.get(name).cloned())
        }

        async fn update(&self, profile: Profile) -> Result<(), DomainError> {
            let mut profiles = self.profiles.lock().unwrap();
            profiles.insert(profile.name.clone(), profile);
            Ok(())
        }

        async fn remove(&self, name: &str) -> Result<(), DomainError> {
            let mut profiles = self.profiles.lock().unwrap();
            profiles.remove(name);
            Ok(())
        }

        async fn list(&self) -> Result<Vec<Profile>, DomainError> {
            let profiles = self.profiles.lock().unwrap();
            Ok(profiles.values().cloned().collect())
        }

        async fn exists(&self, name: &str) -> Result<bool, DomainError> {
            let profiles = self.profiles.lock().unwrap();
            Ok(profiles.contains_key(name))
        }
    }

    #[tokio::test]
    async fn test_add_profile() {
        // Set up dependencies
        let repository = Arc::new(MockProfileRepository::new());
        let event_listener = Arc::new(TestEventListener::new());
        let mut event_bus = EventBus::new();
        event_bus.register(event_listener.clone());
        let service = ProfileService::new(repository.clone(), Arc::new(event_bus));

        // Create a test profile
        let profile = Profile::new("test", "example.com", "user");

        // Add the profile
        service.add_profile(profile.clone()).await.unwrap();

        // Verify the profile was added
        let stored_profile = repository.get(&profile.name).await.unwrap().unwrap();
        assert_eq!(stored_profile.name, profile.name);
        assert_eq!(stored_profile.hostname, profile.hostname);

        // Verify the event was published
        let events = event_listener.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ProfileCreated(p) => {
                assert_eq!(p.name, profile.name);
            }
            _ => panic!("Expected ProfileCreated event"),
        }
    }
}