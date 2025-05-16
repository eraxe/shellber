use crate::errors::{Result, ShellBeError};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use std::collections::HashSet;
use regex::Regex;

/// Plugin security validator to ensure plugins are safe to load
pub struct PluginSecurityValidator {
    max_file_size: u64,
    allowed_imports: HashSet<String>,
    suspicious_patterns: Vec<Regex>,
    enabled: bool,
}

impl Default for PluginSecurityValidator {
    fn default() -> Self {
        // Create a basic validator with default settings
        let mut allowed_imports = HashSet::new();

        // Common safe libraries
        allowed_imports.insert("std".to_string());
        allowed_imports.insert("shellbe_plugin_sdk".to_string());
        allowed_imports.insert("serde".to_string());
        allowed_imports.insert("serde_json".to_string());
        allowed_imports.insert("async_trait".to_string());
        allowed_imports.insert("chrono".to_string());
        allowed_imports.insert("tokio".to_string());
        allowed_imports.insert("clap".to_string());
        allowed_imports.insert("tracing".to_string());

        // Create patterns for suspicious code
        let suspicious_patterns = vec![
            // System command execution
            Regex::new(r"std::process::Command").unwrap(),
            Regex::new(r"::Command::new").unwrap(),
            // File system operations outside of designated paths
            Regex::new(r"std::fs::remove_").unwrap(),
            Regex::new(r"\.write_all\(").unwrap(),
            // Network access
            Regex::new(r"TcpStream::connect").unwrap(),
            Regex::new(r"reqwest::").unwrap(),
            // Unsafe blocks
            Regex::new(r"unsafe\s+\{").unwrap(),
            // Dynamic code evaluation (if possible in Rust)
            Regex::new(r"eval\(").unwrap(),
            // Shell script execution
            Regex::new(r"sh -c").unwrap(),
            Regex::new(r"bash -c").unwrap(),
            // System environment access
            Regex::new(r"std::env::var").unwrap(),
            // Blocking system operations
            Regex::new(r"\.wait\(\)").unwrap(),
        ];

        Self {
            max_file_size: 10 * 1024 * 1024, // 10MB
            allowed_imports,
            suspicious_patterns,
            enabled: true,
        }
    }
}

impl PluginSecurityValidator {
    /// Create a new security validator with custom settings
    pub fn new(max_file_size: u64, allowed_imports: HashSet<String>, enabled: bool) -> Self {
        let mut validator = Self::default();
        validator.max_file_size = max_file_size;

        // Add custom allowed imports
        for import in allowed_imports {
            validator.allowed_imports.insert(import);
        }

        validator.enabled = enabled;
        validator
    }

    /// Check if a library file is too large
    fn check_file_size(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path)
            .map_err(|e| ShellBeError::Security(format!("Failed to get metadata for {}: {}", path.display(), e)))?;

        if metadata.len() > self.max_file_size {
            return Err(ShellBeError::Security(format!(
                "Plugin is too large: {} bytes (max: {} bytes)",
                metadata.len(),
                self.max_file_size
            )));
        }

        Ok(())
    }

    /// Check if the plugin source contains suspicious patterns
    /// This is a basic approximation since we don't have the source code
    fn check_suspicious_patterns(&self, path: &Path) -> Result<Vec<String>> {
        // We can't reliably analyze binary files for patterns
        // For a more thorough approach, we'd need the source code
        // This is a best-effort check using strings utility if available

        let mut suspicious_findings = Vec::new();

        // Try to use the 'strings' utility if available
        if let Ok(output) = Command::new("strings")
            .arg(path.to_str().unwrap_or(""))
            .output()
        {
            if output.status.success() {
                let content = String::from_utf8_lossy(&output.stdout);

                // Check for suspicious patterns
                for pattern in &self.suspicious_patterns {
                    for line in content.lines() {
                        if pattern.is_match(line) {
                            suspicious_findings.push(format!("Suspicious pattern found: {}", line));
                        }
                    }
                }
            }
        }

        Ok(suspicious_findings)
    }

    /// Try to check for risky imports
    fn check_imports(&self, path: &Path) -> Result<Vec<String>> {
        // This is a best-effort check since we don't have the source code
        let mut suspicious_imports = Vec::new();

        // Try to use the 'nm' utility if available on Unix
        #[cfg(unix)]
        if let Ok(output) = Command::new("nm")
            .arg("-D")
            .arg(path.to_str().unwrap_or(""))
            .output()
        {
            if output.status.success() {
                let content = String::from_utf8_lossy(&output.stdout);

                // Look for external symbols
                for line in content.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let symbol = parts[2];
                        let is_allowed = self.allowed_imports.iter()
                            .any(|allowed| symbol.contains(allowed));

                        if !is_allowed && !symbol.starts_with("_") {
                            suspicious_imports.push(format!("Suspicious import: {}", symbol));
                        }
                    }
                }
            }
        }

        Ok(suspicious_imports)
    }

    /// Validate a plugin library
    pub fn validate(&self, path: &Path) -> Result<()> {
        if !self.enabled {
            tracing::warn!("Plugin security validation is disabled. This is not recommended.");
            return Ok(());
        }

        // Basic file checks
        self.check_file_size(path)?;

        // Get suspicious patterns and imports
        let suspicious_patterns = self.check_suspicious_patterns(path)?;
        let suspicious_imports = self.check_imports(path)?;

        // Combine all findings
        let mut all_findings = Vec::new();
        all_findings.extend(suspicious_patterns);
        all_findings.extend(suspicious_imports);

        // For now, just log warnings instead of failing
        // In a production environment, you might want to fail for critical issues
        for finding in &all_findings {
            tracing::warn!("Security finding in plugin {}: {}", path.display(), finding);
        }

        // If there are high-risk findings, fail the validation
        if all_findings.iter().any(|f| f.contains("Suspicious pattern found: unsafe")) {
            return Err(ShellBeError::Security(format!(
                "Plugin contains potentially unsafe code: {}",
                path.display()
            )));
        }

        Ok(())
    }

    /// Set validation enabled/disabled
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Add an allowed import
    pub fn add_allowed_import(&mut self, import: &str) {
        self.allowed_imports.insert(import.to_string());
    }

    /// Set the maximum file size
    pub fn set_max_file_size(&mut self, max_size: u64) {
        self.max_file_size = max_size;
    }
}