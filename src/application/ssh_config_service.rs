use crate::domain::{
    Profile, SshConfigRepository, DomainError,
};
use std::sync::Arc;
use chrono::Utc;

/// Service for managing SSH config integration
pub struct SshConfigService {
    repository: Arc<dyn SshConfigRepository>,
}

impl SshConfigService {
    /// Create a new SshConfigService with the provided repository
    pub fn new(repository: Arc<dyn SshConfigRepository>) -> Self {
        Self {
            repository,
        }
    }

    /// Import profiles from SSH config file
    pub async fn import_profiles(&self) -> Result<Vec<Profile>, DomainError> {
        self.repository.import().await
    }

    /// Export profiles to SSH config file
    pub async fn export_profiles(&self, profiles: &[Profile], replace: bool) -> Result<(), DomainError> {
        self.repository.export(profiles, replace).await
    }

    /// Add a single profile to SSH config
    pub async fn add_profile_to_ssh_config(&self, profile: &Profile) -> Result<(), DomainError> {
        self.repository.add_profile(profile).await
    }

    /// Remove a profile from SSH config
    pub async fn remove_profile_from_ssh_config(&self, profile_name: &str) -> Result<(), DomainError> {
        self.repository.remove_profile(profile_name).await
    }

    /// Create a profile from SSH config format
    pub fn create_profile_from_ssh_config(
        name: &str,
        hostname: &str,
        user: Option<&str>,
        port: Option<u16>,
        identity_file: Option<&str>,
        options: Vec<(String, String)>,
    ) -> Profile {
        let mut profile = Profile::new(
            name,
            hostname,
            user.unwrap_or(""),
        );

        if let Some(port) = port {
            profile.port = port;
        }

        if let Some(identity) = identity_file {
            profile.identity_file = Some(identity.into());
        }

        for (key, value) in options {
            profile.options.insert(key, value);
        }

        profile
    }

    /// Format a profile for SSH config output
    pub fn format_profile_for_ssh_config(&self, profile: &Profile) -> String {
        let mut output = format!("Host {}\n", profile.name);
        output.push_str(&format!("    HostName {}\n", profile.hostname));
        output.push_str(&format!("    User {}\n", profile.username));

        if profile.port != 22 {
            output.push_str(&format!("    Port {}\n", profile.port));
        }

        if let Some(identity) = &profile.identity_file {
            output.push_str(&format!("    IdentityFile {}\n", identity.display()));
        }

        for (key, value) in &profile.options {
            output.push_str(&format!("    {} {}\n", key, value));
        }

        // Add a comment with shellbe metadata
        output.push_str(&format!("    # Added by ShellBe on {}\n", Utc::now().format("%Y-%m-%d %H:%M:%S")));

        output.push('\n');

        output
    }
}