use crate::domain::{
    Alias, AliasRepository, ProfileRepository,
    DomainError,
};
use std::sync::Arc;

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
        match self.alias_repository.get_target(name).await? {
            Some(target) => Ok(target),
            None => {
                // Check if it's a profile instead
                if self.profile_repository.exists(name).await? {
                    Ok(name.to_string())
                } else {
                    Err(DomainError::ProfileNotFound(name.to_string()))
                }
            }
        }
    }
}