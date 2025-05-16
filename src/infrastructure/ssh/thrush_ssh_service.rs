use crate::domain::{Profile, SshService, DomainError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::fs;
use std::time::Duration;
use std::process::{Command, Stdio};
use std::io::Write;

use tokio::time::timeout;
use thrussh::client::{self, Config};
use thrussh::ChannelId;
use thrussh_keys::key::{self, KeyPair, PublicKey};
use thrussh_keys::agent;
use std::sync::Arc;
use futures::future::BoxFuture;

/// Tokio-based implementation of the SSH service
pub struct ThrushSshService {
    client_config: Config,
}

impl ThrushSshService {
    /// Create a new SSH service
    pub fn new() -> Self {
        let mut client_config = Config::default();
        client_config.connection_timeout = Some(Duration::from_secs(10));
        client_config.authenticate_timeout = Some(Duration::from_secs(10));

        Self {
            client_config,
        }
    }

    // Helper function to load SSH keys
    async fn load_key(&self, path: &Path) -> Result<KeyPair, DomainError> {
        let key_data = tokio::fs::read(path).await
            .map_err(|e| DomainError::IoError(e))?;

        match key::parse_secret_key(&key_data, None) {
            Ok(key_pair) => Ok(key_pair),
            Err(_) => {
                // Try with empty passphrase
                key::parse_secret_key(&key_data, Some(b""))
                    .map_err(|e| DomainError::SshError(format!("Failed to load key: {}", e)))
            }
        }
    }

    // Create a pure-Rust SSH key pair
    async fn create_key_pair(&self, key_path: &Path, key_type: &str, comment: Option<&str>) -> Result<(), DomainError> {
        match key_type {
            "ed25519" => {
                let key_pair = KeyPair::generate_ed25519()
                    .map_err(|e| DomainError::SshError(format!("Failed to generate key: {}", e)))?;

                // Save private key
                let private_key = key_pair.serialize_openssh()
                    .map_err(|e| DomainError::SshError(format!("Failed to serialize private key: {}", e)))?;

                tokio::fs::write(key_path, private_key).await
                    .map_err(|e| DomainError::IoError(e))?;

                // Set permissions on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = tokio::fs::metadata(key_path).await.map_err(|e| DomainError::IoError(e))?;
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o600);
                    tokio::fs::set_permissions(key_path, permissions).await.map_err(|e| DomainError::IoError(e))?;
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

                tokio::fs::write(pubkey_path, pubkey_content).await
                    .map_err(|e| DomainError::IoError(e))?;

                Ok(())
            },
            "rsa" => {
                let key_pair = KeyPair::generate_rsa(3072)
                    .map_err(|e| DomainError::SshError(format!("Failed to generate key: {}", e)))?;

                // Save private key
                let private_key = key_pair.serialize_openssh()
                    .map_err(|e| DomainError::SshError(format!("Failed to serialize private key: {}", e)))?;

                tokio::fs::write(key_path, private_key).await
                    .map_err(|e| DomainError::IoError(e))?;

                // Set permissions on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = tokio::fs::metadata(key_path).await.map_err(|e| DomainError::IoError(e))?;
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o600);
                    tokio::fs::set_permissions(key_path, permissions).await.map_err(|e| DomainError::IoError(e))?;
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

                tokio::fs::write(pubkey_path, pubkey_content).await
                    .map_err(|e| DomainError::IoError(e))?;

                Ok(())
            },
            _ => Err(DomainError::SshError(format!("Unsupported key type: {}", key_type))),
        }
    }
}

// SSH client handler
struct ClientHandler {
    success: bool,
    finish_on_session: bool,
}

impl ClientHandler {
    fn new(finish_on_session: bool) -> Self {
        Self {
            success: false,
            finish_on_session,
        }
    }
}

impl client::Handler for ClientHandler {
    type Error = thrussh::Error;
    type FutureUnit = BoxFuture<'static, Result<(Self, client::Session), Self::Error>>;
    type FutureBool = BoxFuture<'static, Result<(Self, bool), Self::Error>>;

    fn finished_bool(self, b: bool) -> Self::FutureBool {
        Box::pin(async move { Ok((self, b)) })
    }

    fn finished(self, session: client::Session) -> Self::FutureUnit {
        if self.finish_on_session {
            Box::pin(async move { Ok((self, session)) })
        } else {
            // Continue the session
            Box::pin(async move { Ok((self, session)) })
        }
    }

    fn check_server_key(self, _server_public_key: &PublicKey) -> Self::FutureBool {
        // In a production implementation, we would check if this key is in known_hosts
        // For now, we'll just accept it
        Box::pin(async move { Ok((self, true)) })
    }

    fn channel_open_confirmation(
        mut self,
        _channel: ChannelId,
        _max_packet_size: u32,
        _window_size: u32,
        session: client::Session,
    ) -> Self::FutureUnit {
        self.success = true;
        Box::pin(async move { Ok((self, session)) })
    }
}

#[async_trait]
impl SshService for ThrushSshService {
    /// Connect to a profile
    async fn connect(&self, profile: &Profile) -> Result<i32, DomainError> {
        // For interactive sessions, we still need to use system SSH
        // thrussh doesn't handle terminal properly for fully interactive sessions
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

        // Set stdin/stdout/stderr for interactive use
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

    /// Test connection to a profile using thrussh
    async fn test_connection(&self, profile: &Profile) -> Result<bool, DomainError> {
        // Use thrussh for connection testing
        let socket_addr = format!("{}:{}", profile.hostname, profile.port);
        let addr = socket_addr.parse()
            .map_err(|e| DomainError::SshError(format!("Invalid address: {}", e)))?;

        // Try to connect with timeout
        match timeout(Duration::from_secs(10), thrussh::client::connect(self.client_config.clone(), addr, ClientHandler::new(true))).await {
            Ok(Ok((_, session))) => {
                // Successfully connected to SSH server
                // In a real implementation, we would also attempt to authenticate
                Ok(true)
            },
            Ok(Err(e)) => {
                // Connection error
                tracing::debug!("SSH connection error: {}", e);
                Ok(false)
            },
            Err(_) => {
                // Timeout
                tracing::debug!("SSH connection timeout");
                Ok(false)
            }
        }
    }

    /// Copy SSH key to a remote server
    async fn copy_key(&self, profile: &Profile, key_path: &Path) -> Result<(), DomainError> {
        // This is complex to implement purely in Rust
        // For now, we'll use ssh-copy-id but provide better error handling
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
            tokio::fs::create_dir_all(&ssh_dir).await
                .map_err(|e| DomainError::IoError(e))?;

            // Set permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = tokio::fs::metadata(&ssh_dir).await.map_err(|e| DomainError::IoError(e))?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o700);
                tokio::fs::set_permissions(&ssh_dir, permissions).await.map_err(|e| DomainError::IoError(e))?;
            }
        }

        let key_path = ssh_dir.join(key_name);
        let pubkey_path = ssh_dir.join(format!("{}.pub", key_name));

        // Check if file already exists
        if key_path.exists() {
            return Err(DomainError::ConfigError(format!("Key file already exists: {}", key_path.display())));
        }

        // Determine key type from name or use default
        let key_type = if key_name.contains("ed25519") {
            "ed25519"
        } else {
            "rsa"  // Default to RSA
        };

        // Create the key pair
        self.create_key_pair(&key_path, key_type, comment).await?;

        Ok((key_path, pubkey_path))
    }
}