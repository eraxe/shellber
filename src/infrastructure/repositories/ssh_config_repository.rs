use crate::domain::{Profile, SshConfigRepository, DomainError};
use crate::utils::{backup_file, ensure_directory, ensure_file};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::{Read, Write, BufRead, BufReader};
use chrono::Utc;
use regex::Regex;

/// File-based implementation of the SSH config repository
pub struct FileSshConfigRepository {
    ssh_config_path: PathBuf,
}

impl FileSshConfigRepository {
    /// Create a new SSH config repository
    pub fn new(ssh_config_path: impl Into<PathBuf>) -> Self {
        Self {
            ssh_config_path: ssh_config_path.into(),
        }
    }

    /// Create SSH config file if it doesn't exist
    async fn ensure_config_file(&self) -> Result<(), DomainError> {
        let ssh_dir = self.ssh_config_path.parent()
            .ok_or_else(|| DomainError::ConfigError("Invalid SSH config path".to_string()))?;

        // Create SSH directory if it doesn't exist
        ensure_directory(ssh_dir).await
            .map_err(|e| DomainError::IoError(e))?;

        // Create SSH config file if it doesn't exist
        ensure_file(&self.ssh_config_path, Some("# SSH config managed by ShellBe\n\n")).await
            .map_err(|e| DomainError::IoError(e))?;

        Ok(())
    }

    /// Create a backup of the SSH config file
    async fn backup_config(&self) -> Result<PathBuf, DomainError> {
        if !self.ssh_config_path.exists() {
            return Ok(self.ssh_config_path.clone());
        }

        backup_file(&self.ssh_config_path).await
            .map_err(|e| DomainError::IoError(e))
    }

    /// Parse SSH config file and extract profiles
    fn parse_config(&self) -> Result<Vec<Profile>, DomainError> {
        if !self.ssh_config_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.ssh_config_path)
            .map_err(|e| DomainError::IoError(e))?;

        let reader = BufReader::new(file);
        let mut profiles = Vec::new();
        let mut current_host: Option<String> = None;
        let mut hostname: Option<String> = None;
        let mut username: Option<String> = None;
        let mut port: u16 = 22;
        let mut identity_file: Option<String> = None;
        let mut options: Vec<(String, String)> = Vec::new();
        let mut in_match_block = false;
        let mut in_conditional = false;

        for line in reader.lines() {
            let line = line.map_err(|e| DomainError::IoError(e))?;
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Convert to lowercase for matching, but preserve case for values
            let line_lower = line.to_lowercase();

            // Handle Match and conditional blocks - we skip these
            if line_lower.starts_with("match ") {
                in_match_block = true;
                continue;
            } else if in_match_block && !line_lower.starts_with("host ") {
                continue;
            } else if line_lower.contains("if ") || line_lower.contains("elseif ") || line_lower.contains("else") {
                in_conditional = true;
                continue;
            } else if in_conditional && line_lower.contains("endif") {
                in_conditional = false;
                continue;
            } else if in_conditional {
                continue;
            }

            // Exit match block when we see a new Host
            if line_lower.starts_with("host ") {
                in_match_block = false;
            }

            if line_lower.starts_with("host ") {
                // Save previous host if we had one
                if let Some(host) = current_host.take() {
                    if let Some(hostname_val) = hostname.take() {
                        // Create profile but only if we have both host and hostname
                        let mut profile = Profile::new(
                            host,
                            hostname_val,
                            username.take().unwrap_or_else(|| whoami::username()),
                        );

                        profile.port = port;

                        if let Some(identity) = identity_file.take() {
                            profile.identity_file = Some(PathBuf::from(shellexpand::tilde(&identity).into_owned()));
                        }

                        for (key, value) in options.drain(..) {
                            profile.options.insert(key, value);
                        }

                        profiles.push(profile);
                    }
                }

                // Reset for new host
                current_host = None;
                hostname = None;
                username = None;
                port = 22;
                identity_file = None;
                options.clear();

                // Parse host value - handle multiple hosts and patterns
                let host_value = line[5..].trim();

                // If multiple hosts, split by whitespace and take first
                let host_parts: Vec<&str> = host_value.split_whitespace().collect();

                // Skip wildcards/patterns and multiple hosts
                if host_parts.len() == 1 && !host_parts[0].contains('*') && !host_parts[0].contains('?') && !host_parts[0].contains('%') {
                    current_host = Some(host_parts[0].to_string());
                }
            } else if let Some(_) = current_host.as_ref() {
                // Parse host properties - handle more complex whitespace formats
                let parts: Vec<&str> = line.splitn(2, |c: char| c.is_whitespace()).collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let value = parts[1].trim();

                    // Handle keys case-insensitively for matching
                    match key.to_lowercase().as_str() {
                        "hostname" => hostname = Some(value.to_string()),
                        "user" => username = Some(value.to_string()),
                        "port" => port = value.parse().unwrap_or(22),
                        "identityfile" => identity_file = Some(value.to_string()),
                        // Other options - preserve original key case
                        _ => options.push((key.to_string(), value.to_string())),
                    }
                }
            }
        }

        // Add the last host if we have one
        if let Some(host) = current_host {
            if let Some(hostname_val) = hostname {
                let mut profile = Profile::new(
                    host,
                    hostname_val,
                    username.unwrap_or_else(|| whoami::username()),
                );

                profile.port = port;

                if let Some(identity) = identity_file {
                    profile.identity_file = Some(PathBuf::from(shellexpand::tilde(&identity).into_owned()));
                }

                for (key, value) in options {
                    profile.options.insert(key, value);
                }

                profiles.push(profile);
            }
        }

        Ok(profiles)
    }

    /// Format a profile for SSH config output
    fn format_profile(&self, profile: &Profile) -> String {
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
            // Capitalize first letter of key for SSH config format
            let key = key.chars().next().map(|c| c.to_uppercase().collect::<String>())
                .unwrap_or_default() + &key[1..];

            output.push_str(&format!("    {} {}\n", key, value));
        }

        // Add a comment with shellbe metadata
        output.push_str(&format!("    # Added by ShellBe on {}\n", Utc::now().format("%Y-%m-%d %H:%M:%S")));
        output.push('\n');

        output
    }

    /// Check if a profile exists in SSH config
    async fn profile_exists_in_config(&self, profile_name: &str) -> Result<bool, DomainError> {
        if !self.ssh_config_path.exists() {
            return Ok(false);
        }

        let file = File::open(&self.ssh_config_path)
            .map_err(|e| DomainError::IoError(e))?;

        let reader = BufReader::new(file);

        // Make sure we handle both exact profile names and profiles that are part of multi-host entries
        let host_regex = Regex::new(&format!(r"^Host\s+{}(\s|$)", regex::escape(profile_name)))
            .map_err(|_| DomainError::ConfigError("Invalid regex".to_string()))?;

        let multi_host_regex = Regex::new(&format!(r"^Host\s+.*\s+{}(\s|$)", regex::escape(profile_name)))
            .map_err(|_| DomainError::ConfigError("Invalid regex".to_string()))?;

        for line in reader.lines() {
            let line = line.map_err(|e| DomainError::IoError(e))?;
            let line_trimmed = line.trim();

            if host_regex.is_match(line_trimmed) || multi_host_regex.is_match(line_trimmed) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[async_trait]
impl SshConfigRepository for FileSshConfigRepository {
    /// Import profiles from SSH config
    async fn import(&self) -> Result<Vec<Profile>, DomainError> {
        self.ensure_config_file().await?;
        self.parse_config()
    }

    /// Export profiles to SSH config
    async fn export(&self, profiles: &[Profile], replace: bool) -> Result<(), DomainError> {
        self.ensure_config_file().await?;

        // Create a backup
        let backup_path = self.backup_config().await?;

        // If replacing, just write new config
        if replace {
            let mut file = File::create(&self.ssh_config_path)
                .map_err(|e| DomainError::IoError(e))?;

            writeln!(file, "# SSH config generated by ShellBe on {}", Utc::now().format("%Y-%m-%d %H:%M:%S"))
                .map_err(|e| DomainError::IoError(e))?;
            writeln!(file, "# Original config backed up to {}", backup_path.display())
                .map_err(|e| DomainError::IoError(e))?;
            writeln!(file).map_err(|e| DomainError::IoError(e))?;

            for profile in profiles {
                write!(file, "{}", self.format_profile(profile))
                    .map_err(|e| DomainError::IoError(e))?;
            }
        } else {
            // Otherwise, append to existing config
            let mut content = String::new();
            if self.ssh_config_path.exists() {
                let mut file = File::open(&self.ssh_config_path)
                    .map_err(|e| DomainError::IoError(e))?;
                file.read_to_string(&mut content)
                    .map_err(|e| DomainError::IoError(e))?;
            }

            let mut file = File::create(&self.ssh_config_path)
                .map_err(|e| DomainError::IoError(e))?;

            // Write existing content
            write!(file, "{}", content).map_err(|e| DomainError::IoError(e))?;

            // Add separator if there's existing content
            if !content.trim().is_empty() {
                writeln!(file).map_err(|e| DomainError::IoError(e))?;
            }

            writeln!(file, "# ShellBe profiles added on {}", Utc::now().format("%Y-%m-%d %H:%M:%S"))
                .map_err(|e| DomainError::IoError(e))?;
            writeln!(file).map_err(|e| DomainError::IoError(e))?;

            // Write profiles
            for profile in profiles {
                write!(file, "{}", self.format_profile(profile))
                    .map_err(|e| DomainError::IoError(e))?;
            }
        }

        // Set proper permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&self.ssh_config_path).map_err(|e| DomainError::IoError(e))?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&self.ssh_config_path, permissions).map_err(|e| DomainError::IoError(e))?;
        }

        Ok(())
    }

    /// Add a single profile to SSH config
    async fn add_profile(&self, profile: &Profile) -> Result<(), DomainError> {
        self.ensure_config_file().await?;

        // Check if profile already exists
        if self.profile_exists_in_config(&profile.name).await? {
            // Remove existing profile
            self.remove_profile(&profile.name).await?;
        }

        // Append to file
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&self.ssh_config_path)
            .map_err(|e| DomainError::IoError(e))?;

        writeln!(file).map_err(|e| DomainError::IoError(e))?;
        write!(file, "{}", self.format_profile(profile))
            .map_err(|e| DomainError::IoError(e))?;

        Ok(())
    }

    /// Remove a profile from SSH config
    async fn remove_profile(&self, profile_name: &str) -> Result<(), DomainError> {
        if !self.ssh_config_path.exists() {
            return Ok(());
        }

        // Check if profile exists
        if !self.profile_exists_in_config(profile_name).await? {
            return Ok(());
        }

        // Create a backup
        self.backup_config().await?;

        // Read file
        let file = File::open(&self.ssh_config_path)
            .map_err(|e| DomainError::IoError(e))?;

        let reader = BufReader::new(file);

        // Create regexes for matching profiles
        let exact_host_regex = Regex::new(&format!(r"^Host\s+{}$", regex::escape(profile_name)))
            .map_err(|_| DomainError::ConfigError("Invalid regex".to_string()))?;

        let multi_host_regex = Regex::new(&format!(r"^Host\s+(.*\s+)?{}(\s+.*)?$", regex::escape(profile_name)))
            .map_err(|_| DomainError::ConfigError("Invalid regex".to_string()))?;

        // Parse file and handle profiles more robustly
        let mut output = Vec::new();
        let mut skip = false;
        let mut in_host_block = false;
        let mut host_block_start = 0;
        let mut i = 0;

        for line in reader.lines() {
            let line = line.map_err(|e| DomainError::IoError(e))?;
            let line_trimmed = line.trim();

            // Detect Host blocks
            if line_trimmed.starts_with("Host ") {
                // End previous host block if any
                if in_host_block {
                    in_host_block = false;
                }

                // Start new host block
                in_host_block = true;
                host_block_start = i;

                // Check if this is our target profile
                if exact_host_regex.is_match(line_trimmed) {
                    // Exact match, skip the whole block
                    skip = true;
                } else if multi_host_regex.is_match(line_trimmed) {
                    // This is a multi-host entry containing our profile
                    // We need to modify the line to remove just this profile
                    let parts: Vec<&str> = line_trimmed[5..].trim().split_whitespace().collect();
                    let new_parts: Vec<&str> = parts.into_iter()
                        .filter(|&p| p != profile_name)
                        .collect();

                    if new_parts.is_empty() {
                        // No hosts left, skip the whole block
                        skip = true;
                    } else {
                        // Rebuild the line with remaining hosts
                        let new_line = format!("Host {}", new_parts.join(" "));
                        output.push(new_line);
                        skip = false;
                    }

                    // Skip the original line since we've handled it
                    i += 1;
                    continue;
                } else {
                    // Not our target, include it
                    skip = false;
                }
            } else if in_host_block && line_trimmed.starts_with("Host ") {
                // New Host block
                in_host_block = false;
                skip = false;
            }

            if !skip {
                output.push(line);
            }

            i += 1;
        }

        // Write back to file
        let mut file = File::create(&self.ssh_config_path)
            .map_err(|e| DomainError::IoError(e))?;

        for line in output {
            writeln!(file, "{}", line).map_err(|e| DomainError::IoError(e))?;
        }

        Ok(())
    }
}