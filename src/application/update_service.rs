use crate::domain::DomainError;
use reqwest::blocking::Client;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

/// Current version of the application
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository owner
const REPO_OWNER: &str = "arash";
/// GitHub repository name
const REPO_NAME: &str = "shellbe";

/// Error type for self-update operations
#[derive(thiserror::Error, Debug)]
pub enum UpdateError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Domain error: {0}")]
    DomainError(#[from] DomainError),

    #[error("Update error: {0}")]
    Other(String),
}

/// Result type for self-update operations
pub type Result<T> = std::result::Result<T, UpdateError>;

/// GitHub release information
#[derive(Debug, serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

/// GitHub release asset
#[derive(Debug, serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Service for handling application self-updates
pub struct UpdateService {
    client: Client,
    current_version: String,
}

impl UpdateService {
    /// Create a new update service
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            current_version: CURRENT_VERSION.to_string(),
        }
    }

    /// Check if an update is available
    pub fn check_for_update(&self) -> Result<Option<String>> {
        let url = format!("https://api.github.com/repos/{}/{}/releases/latest", REPO_OWNER, REPO_NAME);

        let response = self.client
            .get(&url)
            .header("User-Agent", format!("ShellBe/{}", self.current_version))
            .send()?;

        if !response.status().is_success() {
            return Err(UpdateError::Other(format!("Failed to check for updates: {}", response.status())));
        }

        let release: GithubRelease = response.json()?;

        // Compare versions
        let latest_version = release.tag_name.trim_start_matches('v');
        if latest_version != self.current_version {
            return Ok(Some(latest_version.to_string()));
        }

        Ok(None)
    }

    /// Update the application to the latest version
    pub fn update(&self) -> Result<()> {
        // Check if update is available
        let latest_version = match self.check_for_update()? {
            Some(version) => version,
            None => {
                return Err(UpdateError::Other("No update available".to_string()));
            }
        };

        println!("Updating from {} to {}...", self.current_version, latest_version);

        // Use cargo install for the update
        let status = Command::new("cargo")
            .arg("install")
            .arg("--force")
            .arg(REPO_NAME)
            .status()?;

        if !status.success() {
            return Err(UpdateError::Other("Failed to update via cargo install".to_string()));
        }

        println!("Update completed successfully!");

        Ok(())
    }

    /// Get the path to the current executable
    pub fn executable_path() -> Result<PathBuf> {
        let exe = env::current_exe()
            .map_err(|e| UpdateError::Other(format!("Failed to get executable path: {}", e)))?;

        Ok(exe)
    }

    /// Create a backup of the current executable
    pub fn backup_executable(&self) -> Result<PathBuf> {
        let exe_path = Self::executable_path()?;
        let backup_path = exe_path.with_extension(format!("backup.{}", chrono::Utc::now().format("%Y%m%d%H%M%S")));

        fs::copy(&exe_path, &backup_path)?;

        Ok(backup_path)
    }
}