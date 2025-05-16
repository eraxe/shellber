use crate::domain::{
    Alias, AliasRepository, ProfileRepository,
    DomainError,
};
use std::sync::Arc;
use std::collections::HashSet;

/// AliasService manages SSH connection aliases
pub struct AliasService {
    alias_repository: Arc<dyn AliasRepository>,
    profile_repository: Arc<dyn ProfileRepository>,
}

impl AliasService {
    /// Create a new AliasService with the provided repositories
    pub fn new(
        alias_repository: Arc<dyn AliasRepository>,
        profile_repository: Arc<dyn ProfileRepository>,
    ) -> Self {
        Self {
            alias_repository,
            profile_repository,
        }
    }

    /// Create a new alias for a profile
    pub async fn create_alias(&self, alias_name: &str, profile_name: &str) -> Result<(), DomainError> {
        // Check if profile exists
        if !self.profile_repository.exists(profile_name).await? {
            return Err(DomainError::ProfileNotFound(profile_name.to_string()));
        }

        // Check if target is an alias (to detect potential circular references)
        if let Some(target) = self.alias_repository.get_target(profile_name).await? {
            // The target is an alias itself, check for circular reference
            // Traverse the chain to check for cycles
            let mut visited = HashSet::new();
            visited.insert(alias_name.to_string());
            visited.insert(profile_name.to_string());

            let mut current = target;
            while let Some(next) = self.alias_repository.get_target(&current).await? {
                if visited.contains(&next) {
                    return Err(DomainError::ConfigError(
                        format!("Circular alias reference detected: {} -> {} -> {}",
                                alias_name, profile_name, next)
                    ));
                }
                visited.insert(next.clone());
                current = next;
            }
        }

        // Check if alias already exists
        if let Some(_) = self.alias_repository.get_target(alias_name).await? {
            return Err(DomainError::AliasAlreadyExists(alias_name.to_string()));
        }

        // Create the alias
        let alias = Alias::new(alias_name, profile_name);
        self.alias_repository.add(alias).await?;

        Ok(())
    }

    /// Get all aliases
    pub async fn list_aliases(&self) -> Result<Vec<Alias>, DomainError> {
        self.alias_repository.list().await
    }

    /// Remove an alias
    pub async fn remove_alias(&self, alias_name: &str) -> Result<(), DomainError> {
        // Check if alias exists
        if let None = self.alias_repository.get_target(alias_name).await? {
            return Err(DomainError::AliasNotFound(alias_name.to_string()));
        }

        // Remove the alias
        self.alias_repository.remove(alias_name).await?;

        Ok(())
    }

    /// Get aliases for a specific profile
    pub async fn get_aliases_for_profile(&self, profile_name: &str) -> Result<Vec<Alias>, DomainError> {
        // Check if profile exists
        if !self.profile_repository.exists(profile_name).await? {
            return Err(DomainError::ProfileNotFound(profile_name.to_string()));
        }

        // Get aliases for the profile
        self.alias_repository.list_for_profile(profile_name).await
    }

    /// Resolve an alias to a profile name
    pub async fn resolve_alias(&self, name: &str) -> Result<String, DomainError> {
        let mut visited = HashSet::new();
        visited.insert(name.to_string());

        let mut current = name.to_string();

        // Follow the alias chain until we reach a profile
        while let Some(target) = self.alias_repository.get_target(&current).await? {
            // Check for cycles
            if visited.contains(&target) {
                return Err(DomainError::ConfigError(
                    format!("Circular alias reference detected: {} -> {}", current, target)
                ));
            }

            visited.insert(target.clone());
            current = target;
        }

        // Check if the final target is a valid profile
        if self.profile_repository.exists(&current).await? {
            Ok(current)
        } else {
            Err(DomainError::ProfileNotFound(current))
        }
    }

    /// Check if a name is an alias
    pub async fn is_alias(&self, name: &str) -> Result<bool, DomainError> {
        Ok(self.alias_repository.get_target(name).await?.is_some())
    }
}