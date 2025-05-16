use thiserror::Error;
use std::io;
use std::path::PathBuf;

/// Unified error type for ShellBe
#[derive(Error, Debug, Clone)]
pub enum ShellBeError {
    #[error("Profile error: {0}")]
    Profile(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("SSH error: {0}")]
    Ssh(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("File lock error: {0}")]
    FileLock(String),

    #[error("Update error: {0}")]
    Update(String),

    #[error("System requirement error: {0}")]
    SystemRequirement(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),
}

// Implement From for common error types
impl From<io::Error> for ShellBeError {
    fn from(error: io::Error) -> Self {
        ShellBeError::Io(error.to_string())
    }
}

impl From<serde_json::Error> for ShellBeError {
    fn from(error: serde_json::Error) -> Self {
        ShellBeError::Config(format!("JSON error: {}", error))
    }
}

impl From<reqwest::Error> for ShellBeError {
    fn from(error: reqwest::Error) -> Self {
        ShellBeError::Update(format!("Network error: {}", error))
    }
}

impl From<libloading::Error> for ShellBeError {
    fn from(error: libloading::Error) -> Self {
        ShellBeError::Plugin(format!("Library loading error: {}", error))
    }
}

// Result type alias for ShellBe
pub type Result<T> = std::result::Result<T, ShellBeError>;

// Helper functions for error context
pub trait ErrorContext<T> {
    fn with_context<C>(self, context: C) -> Result<T>
    where
        C: FnOnce() -> String;
}

impl<T, E: Into<ShellBeError>> ErrorContext<T> for std::result::Result<T, E> {
    fn with_context<C>(self, context: C) -> Result<T>
    where
        C: FnOnce() -> String,
    {
        self.map_err(|err| {
            let shell_be_err = err.into();
            match shell_be_err {
                ShellBeError::Io(msg) => ShellBeError::Io(format!("{}: {}", context(), msg)),
                ShellBeError::Config(msg) => ShellBeError::Config(format!("{}: {}", context(), msg)),
                ShellBeError::Plugin(msg) => ShellBeError::Plugin(format!("{}: {}", context(), msg)),
                ShellBeError::Ssh(msg) => ShellBeError::Ssh(format!("{}: {}", context(), msg)),
                ShellBeError::Connection(msg) => ShellBeError::Connection(format!("{}: {}", context(), msg)),
                ShellBeError::Profile(msg) => ShellBeError::Profile(format!("{}: {}", context(), msg)),
                ShellBeError::Security(msg) => ShellBeError::Security(format!("{}: {}", context(), msg)),
                ShellBeError::FileLock(msg) => ShellBeError::FileLock(format!("{}: {}", context(), msg)),
                ShellBeError::Update(msg) => ShellBeError::Update(format!("{}: {}", context(), msg)),
                ShellBeError::SystemRequirement(msg) => ShellBeError::SystemRequirement(format!("{}: {}", context(), msg)),
                ShellBeError::NotFound(msg) => ShellBeError::NotFound(format!("{}: {}", context(), msg)),
                ShellBeError::AlreadyExists(msg) => ShellBeError::AlreadyExists(format!("{}: {}", context(), msg)),
            }
        })
    }
}

// Conversion from domain errors
impl From<crate::domain::Error> for ShellBeError {
    fn from(error: crate::domain::Error) -> Self {
        match error {
            crate::domain::Error::ProfileNotFound(name) => ShellBeError::NotFound(format!("Profile not found: {}", name)),
            crate::domain::Error::ProfileAlreadyExists(name) => ShellBeError::AlreadyExists(format!("Profile already exists: {}", name)),
            crate::domain::Error::AliasNotFound(name) => ShellBeError::NotFound(format!("Alias not found: {}", name)),
            crate::domain::Error::AliasAlreadyExists(name) => ShellBeError::AlreadyExists(format!("Alias already exists: {}", name)),
            crate::domain::Error::SshError(msg) => ShellBeError::Ssh(msg),
            crate::domain::Error::IoError(err) => ShellBeError::Io(err.to_string()),
            crate::domain::Error::ConfigError(msg) => ShellBeError::Config(msg),
        }
    }
}

// Conversions from other error types in the codebase
impl From<crate::application::PluginError> for ShellBeError {
    fn from(error: crate::application::PluginError) -> Self {
        match error {
            crate::application::PluginError::NotFound(name) => ShellBeError::NotFound(format!("Plugin not found: {}", name)),
            crate::application::PluginError::AlreadyExists(name) => ShellBeError::AlreadyExists(format!("Plugin already exists: {}", name)),
            crate::application::PluginError::LoadFailed(msg) => ShellBeError::Plugin(format!("Failed to load plugin: {}", msg)),
            crate::application::PluginError::ApiVersionMismatch => ShellBeError::Plugin("Plugin API version mismatch".to_string()),
            crate::application::PluginError::SecurityValidationFailed(msg) => ShellBeError::Security(msg),
            crate::application::PluginError::DownloadFailed(msg) => ShellBeError::Update(format!("Download failed: {}", msg)),
            crate::application::PluginError::InstallationFailed(msg) => ShellBeError::Plugin(format!("Installation failed: {}", msg)),
            crate::application::PluginError::IoError(err) => ShellBeError::Io(err.to_string()),
            crate::application::PluginError::HttpError(err) => ShellBeError::Update(format!("HTTP error: {}", err)),
            crate::application::PluginError::LibraryError(msg) => ShellBeError::Plugin(format!("Library error: {}", msg)),
            crate::application::PluginError::DomainError(err) => err.into(),
            crate::application::PluginError::LockError(msg) => ShellBeError::FileLock(msg),
        }
    }
}

impl From<crate::application::UpdateError> for ShellBeError {
    fn from(error: crate::application::UpdateError) -> Self {
        match error {
            crate::application::UpdateError::HttpError(err) => ShellBeError::Update(format!("HTTP error: {}", err)),
            crate::application::UpdateError::IoError(err) => ShellBeError::Io(err.to_string()),
            crate::application::UpdateError::JsonError(err) => ShellBeError::Config(format!("JSON error: {}", err)),
            crate::application::UpdateError::DomainError(err) => err.into(),
            crate::application::UpdateError::Other(msg) => ShellBeError::Update(msg),
        }
    }
}

// Function to check if file exists and is accessible
pub fn check_file_exists_and_accessible(path: &PathBuf) -> Result<()> {
    if !path.exists() {
        return Err(ShellBeError::NotFound(format!("File not found: {}", path.display())));
    }

    // Try to open the file to check if it's accessible
    let file = std::fs::File::open(path)
        .map_err(|e| ShellBeError::Io(format!("Cannot access file {}: {}", path.display(), e)))?;

    // Check if it's a file
    let metadata = file.metadata()
        .map_err(|e| ShellBeError::Io(format!("Cannot get metadata for {}: {}", path.display(), e)))?;

    if !metadata.is_file() {
        return Err(ShellBeError::Io(format!("{} is not a file", path.display())));
    }

    Ok(())
}

// Function to check if directory exists and is accessible
pub fn check_directory_exists_and_accessible(path: &PathBuf) -> Result<()> {
    if !path.exists() {
        return Err(ShellBeError::NotFound(format!("Directory not found: {}", path.display())));
    }

    // Try to read the directory to check if it's accessible
    std::fs::read_dir(path)
        .map_err(|e| ShellBeError::Io(format!("Cannot access directory {}: {}", path.display(), e)))?;

    // Check if it's a directory
    let metadata = std::fs::metadata(path)
        .map_err(|e| ShellBeError::Io(format!("Cannot get metadata for {}: {}", path.display(), e)))?;

    if !metadata.is_dir() {
        return Err(ShellBeError::Io(format!("{} is not a directory", path.display())));
    }

    Ok(())
}