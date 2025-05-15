use crate::domain::{Profile, SshService, DomainError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::Write;
use std::fs;
use std::time::Duration;
use tokio::time::timeout;
use thrussh::client::{self, Connect};
use thrussh_keys::key::KeyPair;
use futures::future::BoxFuture;

/// Tokio-based implementation of the SSH service
pub struct ThrushSshService {
    client_config: client::Config,
}

impl ThrushSshService {
    /// Create a new SSH service
    pub fn new() -> Self {
        let mut client_config = client::Config::default();
        client_config.connection_timeout = Some(Duration::from_secs(10));
        client_config.authenticate_timeout = Some(Duration::from_secs(10));

        Self {
            client_config,
        }
    }

    /// Create an SSH key pair using ssh-keygen
    fn create_key_with_ssh_keygen(
        &self,
        key_path: &Path,
        key_type: &str,
        bits: u32,
        comment: Option<&str>,
    ) -> Result<(), DomainError> {
        let mut cmd = Command::new("ssh-keygen");

        cmd.arg("-t").arg(key_type)
            .arg("-b").arg(bits.to_string())
            .arg("-f").arg(key_path)
            .arg("-N").arg("");  // Empty passphrase

        if let Some(c) = comment {
            cmd.arg("-C").arg(c);
        }

        let status = cmd.spawn()
            .map_err(|e| DomainError::SshError(format!("Failed to execute ssh-keygen: {}", e)))?
            .wait()
            .map_err(|e| DomainError::SshError(format!("Failed to wait for ssh-keygen: {}", e)))?;

        if !status.success() {
            return Err(DomainError::SshError(format!("ssh-keygen returned error: {}", status)));
        }

        Ok(())
    }

    /// Create an SSH key pair using Rust libraries
    fn create_key_with_rust(&self, key_path: &Path, comment: Option<&str>) -> Result<(), DomainError> {
        // Generate key pair
        let key_pair = KeyPair::generate_ed25519()
            .map_err(|e| DomainError::SshError(format!("Failed to generate key: {}", e)))?;

        // Save private key
        let private_key = key_pair.serialize_openssh()
            .map_err(|e| DomainError::SshError(format!("Failed to serialize private key: {}", e)))?;

        fs::write(key_path, private_key)
            .map_err(|e| DomainError::IoError(e))?;

        // Set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(key_path).map_err(|e| DomainError::IoError(e))?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(key_path, permissions).map_err(|e| DomainError::IoError(e))?;
        }

        // Generate public key
        let pubkey_path = format!("{}.pub", key_path.display());
        let pubkey = key_pair.clone_public_key().serialize_openssh()
            .map_err(|e| DomainError::SshError(format!("Failed to serialize public key: {}", e)))?;

        let pubkey_content = if let Some(c) = comment {
            format!("{} {}", pubkey, c)
        } else {
            pubkey
        };

        fs::write(pubkey_path, pubkey_content)
            .map_err(|e| DomainError::IoError(e))?;

        Ok(())
    }

    /// Load an SSH key from a file
    async fn load_key(&self, path: &Path) -> Result<KeyPair, DomainError> {
        let key_data = tokio::fs::read(path).await
            .map_err(|e| DomainError::IoError(e))?;

        thrussh_keys::key::KeyPair::from_openssh(&key_data)
            .map_err(|e| DomainError::SshError(format!("Failed to load key: {}", e)))
    }
}

struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = thrussh::Error;
    type FutureUnit = BoxFuture<'static, Result<(Self, client::Session), Self::Error>>;
    type FutureBool = BoxFuture<'static, Result<(Self, bool), Self::Error>>;

    fn finished_bool(self, b: bool) -> Self::FutureBool {
        Box::pin(async move { Ok((self, b)) })
    }

    fn finished(self, session: client::Session) -> Self::FutureUnit {
        Box::pin(async move { Ok((self, session)) })
    }

    fn check_server_key(self, server_public_key: &thrussh_keys::key::PublicKey) -> Self::FutureBool {
        self.finished_bool(true)
    }
}

#[async_trait]
impl SshService for ThrushSshService {
    /// Connect to a profile
    async fn connect(&self, profile: &Profile) -> Result<i32, DomainError> {
        // For now, use the system SSH command
        // Will implement native SSH later with thrussh
        let mut cmd = Command::new("ssh");

        // Add port if not default
        if profile.port != 22 {
            cmd.arg("-p").arg(profile.port.to_string());
        }

        // Add identity file if specified
        if let Some(identity) = &profile.identity_file {
            cmd.arg("-i").arg(identity);
        }

        // Add any additional options
        for (key, value) in &profile.options {
            cmd.arg(format!("-{}", key)).arg(value);
        }

        // Add the connection string
        cmd.arg(format!("{}@{}", profile.username, profile.hostname));

        // Set stdin/stdout/stderr
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Run the command
        let status = cmd.spawn()
            .map_err(|e| DomainError::SshError(format!("Failed to execute SSH: {}", e)))?
            .wait()
            .map_err(|e| DomainError::SshError(format!("Failed to wait for SSH: {}", e)))?;

        Ok(status.code().unwrap_or(1))
    }

    /// Test connection to a profile
    async fn test_connection(&self, profile: &Profile) -> Result<bool, DomainError> {
        // Using the system SSH command with a timeout
        let mut cmd = Command::new("ssh");

        cmd.arg("-o").arg("ConnectTimeout=5")
            .arg("-o").arg("BatchMode=yes")
            .arg("-o").arg("StrictHostKeyChecking=no");

        // Add port if not default
        if profile.port != 22 {
            cmd.arg("-p").arg(profile.port.to_string());
        }

        // Add identity file if specified
        if let Some(identity) = &profile.identity_file {
            cmd.arg("-i").arg(identity);
        }

        // Add the connection string
        cmd.arg(format!("{}@{}", profile.username, profile.hostname));

        // Add a simple command
        cmd.arg("exit");

        // Run with a timeout
        let status = match timeout(Duration::from_secs(10), cmd.output()).await {
            Ok(result) => match result {
                Ok(output) => output.status.success(),
                Err(_) => false,
            },
            Err(_) => false,  // Timeout
        };

        Ok(status)
    }

    /// Copy SSH key to a remote server
    async fn copy_key(&self, profile: &Profile, key_path: &Path) -> Result<(), DomainError> {
        // Using ssh-copy-id
        let mut cmd = Command::new("ssh-copy-id");

        // Add port if not default
        if profile.port != 22 {
            cmd.arg("-p").arg(profile.port.to_string());
        }

        // Add identity file
        cmd.arg("-i").arg(key_path);

        // Add the connection string
        cmd.arg(format!("{}@{}", profile.username, profile.hostname));

        // Set stdin/stdout/stderr
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Run the command
        let status = cmd.spawn()
            .map_err(|e| DomainError::SshError(format!("Failed to execute ssh-copy-id: {}", e)))?
            .wait()
            .map_err(|e| DomainError::SshError(format!("Failed to wait for ssh-copy-id: {}", e)))?;

        if !status.success() {
            return Err(DomainError::SshError(format!("ssh-copy-id returned error: {}", status)));
        }

        Ok(())
    }

    /// Generate a new SSH key pair
    async fn generate_key(&self, key_name: &str, comment: Option<&str>) -> Result<(PathBuf, PathBuf), DomainError> {
        // Determine paths
        let ssh_dir = dirs::home_dir()
            .ok_or_else(|| DomainError::ConfigError("Could not determine home directory".to_string()))?
            .join(".ssh");

        if !ssh_dir.exists() {
            fs::create_dir_all(&ssh_dir)
                .map_err(|e| DomainError::IoError(e))?;

            // Set permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(&ssh_dir).map_err(|e| DomainError::IoError(e))?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o700);
                fs::set_permissions(&ssh_dir, permissions).map_err(|e| DomainError::IoError(e))?;
            }
        }

        let key_path = ssh_dir.join(key_name);
        let pubkey_path = ssh_dir.join(format!("{}.pub", key_name));

        // Check if file already exists
        if key_path.exists() {
            return Err(DomainError::ConfigError(format!("Key file already exists: {}", key_path.display())));
        }

        // Try to use ssh-keygen first, fall back to Rust implementation
        match self.create_key_with_ssh_keygen(&key_path, "ed25519", 0, comment) {
            Ok(_) => (),
            Err(_) => {
                // Fall back to Rust implementation
                self.create_key_with_rust(&key_path, comment)?;
            }
        }

        Ok((key_path, pubkey_path))
    }
}