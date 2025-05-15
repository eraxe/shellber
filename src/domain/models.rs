use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// SSH profile configuration containing connection details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    /// Unique name/identifier for the profile
    pub name: String,
    /// Hostname or IP address
    pub hostname: String,
    /// Username for SSH login
    pub username: String,
    /// SSH port, defaults to 22
    #[serde(default = "default_port")]
    pub port: u16,
    /// Path to identity file (private key)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<PathBuf>,
    /// Additional SSH options
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
    /// Date the profile was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Date the profile was last modified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Date the profile was last accessed/used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_port() -> u16 {
    22
}

impl Profile {
    /// Create a new SSH profile with default values
    pub fn new(name: impl Into<String>, hostname: impl Into<String>, username: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            name: name.into(),
            hostname: hostname.into(),
            username: username.into(),
            port: default_port(),
            identity_file: None,
            options: HashMap::new(),
            created_at: Some(now),
            updated_at: Some(now),
            last_used: None,
        }
    }

    /// Update the last used timestamp
    pub fn mark_as_used(&mut self) {
        self.last_used = Some(chrono::Utc::now());
    }

    /// Update the last modified timestamp
    pub fn mark_as_updated(&mut self) {
        self.updated_at = Some(chrono::Utc::now());
    }

    /// Get SSH connection string in the format username@hostname
    pub fn connection_string(&self) -> String {
        format!("{}@{}", self.username, self.hostname)
    }

    /// Build SSH command string with all options
    pub fn ssh_command(&self) -> String {
        let mut cmd = String::from("ssh");

        // Add port if not default
        if self.port != 22 {
            cmd.push_str(&format!(" -p {}", self.port));
        }

        // Add identity file if specified
        if let Some(identity) = &self.identity_file {
            cmd.push_str(&format!(" -i {}", identity.display()));
        }

        // Add any additional options
        for (key, value) in &self.options {
            cmd.push_str(&format!(" -{} {}", key, value));
        }

        // Add the connection string
        cmd.push_str(&format!(" {}", self.connection_string()));

        cmd
    }
}

/// An alias points to a profile by name
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Alias {
    /// Alias name
    pub name: String,
    /// Target profile name
    pub target: String,
}

impl Alias {
    pub fn new(name: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            target: target.into(),
        }
    }
}

/// Connection history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Timestamp of the connection
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Profile name used
    pub profile_name: String,
    /// Host connected to
    pub hostname: String,
    /// Exit code of the connection
    pub exit_code: Option<i32>,
    /// Duration of the connection
    pub duration: Option<std::time::Duration>,
}

impl HistoryEntry {
    pub fn new(profile_name: impl Into<String>, hostname: impl Into<String>) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            profile_name: profile_name.into(),
            hostname: hostname.into(),
            exit_code: None,
            duration: None,
        }
    }

    pub fn with_result(mut self, exit_code: i32, duration: std::time::Duration) -> Self {
        self.exit_code = Some(exit_code);
        self.duration = Some(duration);
        self
    }
}

/// Connection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    /// Profile name
    pub profile_name: String,
    /// Number of connections
    pub connection_count: usize,
    /// Total connection time
    pub total_duration: std::time::Duration,
    /// Average connection time
    pub average_duration: std::time::Duration,
    /// Last connection timestamp
    pub last_connection: chrono::DateTime<chrono::Utc>,
}