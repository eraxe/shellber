use crate::domain::{Profile, SshConfigRepository, DomainError};
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
    fn ensure_config_file(&self) -> Result<(), DomainError> {
        let ssh_dir = self.ssh_config_path.parent()
            .ok_or_else(|| DomainError::ConfigError("Invalid SSH config path".to_string()))?;

        // Create SSH directory if it doesn't exist
        if !ssh_dir.exists() {
            fs::create_dir_all(ssh_dir)
                .map_err(|e| DomainError::IoError(e))?;

            // Set proper permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(ssh_dir).map_err(|e| DomainError::IoError(e))?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o700);
                fs::set_permissions(ssh_dir, permissions).map_err(|e| DomainError::IoError(e))?;
            }
        }

        // Create SSH config file if it doesn't exist
        if !self.ssh_config_path.exists() {
            File::create(&self.ssh_config_path)
                .map_err(|e| DomainError::IoError(e))?;

            // Set proper permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(&self.ssh_config_path).map_err(|e| DomainError::IoError(e))?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o600);
                fs::set_permissions(&self.ssh_config_path, permissions).map_err(|e| DomainError::IoError(e))?;
            }
        }

        Ok(())
    }

    /// Create a backup of the SSH config file
    fn backup_config(&self) -> Result<PathBuf, DomainError> {
        if !self.ssh_config_path.exists() {
            return Ok(self.ssh_config_path.clone());
        }

        let backup_path = self.ssh_config_path.with_extension(
            format!("backup.{}", Utc::now().format("%Y%m%d%H%M%S"))
        );

        fs::copy(&self.ssh_config_path, &backup_path)
            .map_err(|e| DomainError::IoError(e))?;

        Ok(backup_path)
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
        let mut current_host = None;
        let mut hostname = None;
        let mut username = None;
        let mut port = 22;
        let mut identity_file = None;
        let mut options = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(|e| DomainError::IoError(e))?;
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Convert to lowercase for matching, but preserve case for values
            let line_lower = line.to_lowercase();

            if line_lower.starts_with("host ") {
                // Save previous host if we had one
                if let Some(host) = current_host.take() {
                    if let Some(hostname_val) = hostname.take() {
                        let profile = Profile::new(
                            host,
                            hostname_val,
                            username.take().unwrap_or_else(|| "".to_string()),
                        );

                        let mut profile = profile;
                        profile.port = port;

                        if let Some(identity) = identity_file.take() {
                            profile.identity_file = Some(PathBuf::from(identity));
                        }

                        for (key, value) in options.drain(..) {
                            profile.options.insert(key, value);
                        }

                        profiles.push(profile);
                    }
                }

                // Start new host
                let host_value = line[5..].trim();

                // Skip wildcard hosts
                if host_value.contains('*') || host_value.contains('?') {
                    continue;
                }

                current_host = Some(host_value.to_string());
                hostname = None;
                username = None;
                port = 22;
                identity_file = None;
                options.clear();
            } else if let Some(_) = current_host.as_ref() {
                // Parse host properties
                let parts: Vec<&str> = line.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_lowercase();
                    let value = parts[1].trim();

                    match key.as_str() {
                        "hostname" => hostname = Some(value.to_string()),
                        "user" => username = Some(value.to_string()),
                        "port" => port = value.parse().unwrap_or(22),
                        "identityfile" => identity_file = Some(value.to_string()),
                        // Other options
                        _ => options.push((key, value.to_string())),
                    }
                }
            }
        }

        // Add the last host if we have one
        if let Some(host) = current_host {
            if let Some(hostname_val) = hostname {
                let profile = Profile::new(
                    host,
                    hostname_val,
                    username.unwrap_or_else(|| "".to_string()),
                );

                let mut profile = profile;
                profile.port = port;

                if let Some(identity) = identity_file {
                    profile.identity_file = Some(PathBuf::from(identity));
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
    fn profile_exists_in_config(&self, profile_name: &str) -> Result<bool, DomainError> {
        if !self.ssh_config_path.exists() {
            return Ok(false);
        }

        let file = File::open(&self.ssh_config_path)
            .map_err(|e| DomainError::IoError(e))?;

        let reader = BufReader::new(file);
        let host_regex = Regex::new(&format!(r"^Host\s+{}$", regex::escape(profile_name)))
            .map_err(|_| DomainError::ConfigError("Invalid regex".to_string()))?;

        for line in reader.lines() {
            let line = line.map_err(|e| DomainError::IoError(e))?;
            if host_regex.is_match(line.trim()) {
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
        self.ensure_config_file()?;
        self.parse_config()
    }

    /// Export profiles to SSH config
    async fn export(&self, profiles: &[Profile], replace: bool) -> Result<(), DomainError> {
        self.ensure_config_file()?;

        // Create a backup
        let backup_path = self.backup_config()?;

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

            // Add separator
            writeln!(file).map_err(|e| DomainError::IoError(e))?;
            writeln!(file, "# ShellBe profiles added on {}", Utc::now().format("%Y-%m-%d %H:%M:%S"))
                .map_err(|e| DomainError::IoError(e))?;
            writeln!(file).map_err(|e| DomainError::IoError(e))?;

            // Write profiles
            for profile in profiles {
                write!(file, "{}", self.format_profile(profile))
                    .map_err(|e| DomainError::IoError(e))?;
            }
        }

        // Set proper permissions
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
        self.ensure_config_file()?;

        // Check if profile already exists
        if self.profile_exists_in_config(&profile.name)? {
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
        if !self.profile_exists_in_config(profile_name)? {
            return Ok(());
        }

        // Create a backup
        self.backup_config()?;

        // Read file
        let file = File::open(&self.ssh_config_path)
            .map_err(|e| DomainError::IoError(e))?;

        let reader = BufReader::new(file);
        let host_regex = Regex::new(&format!(r"^Host\s+{}$", regex::escape(profile_name)))
            .map_err(|_| DomainError::ConfigError("Invalid regex".to_string()))?;

        // Parse file and skip the profile to remove
        let mut output = Vec::new();
        let mut skip = false;

        for line in reader.lines() {
            let line = line.map_err(|e| DomainError::IoError(e))?;

            if host_regex.is_match(line.trim()) {
                skip = true;
                continue;
            } else if skip && line.trim().starts_with("Host ") {
                skip = false;
            }

            if !skip {
                output.push(line);
            }
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