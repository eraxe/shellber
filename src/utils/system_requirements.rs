use crate::errors::{ShellBeError, Result};
use std::process::Command;
use std::path::Path;
use std::collections::HashMap;

/// System requirements checker
pub struct SystemRequirements {
    required_commands: Vec<String>,
    required_libraries: Vec<String>,
    required_directories: Vec<String>,
    min_disk_space_mb: u64,
}

impl Default for SystemRequirements {
    fn default() -> Self {
        let mut required_commands = Vec::new();

        // SSH tools are required
        required_commands.push("ssh".to_string());
        required_commands.push("ssh-keygen".to_string());
        required_commands.push("ssh-copy-id".to_string());

        // Git is used for plugin updates
        required_commands.push("git".to_string());

        Self {
            required_commands,
            required_libraries: Vec::new(),
            required_directories: Vec::new(),
            min_disk_space_mb: 10, // Minimal requirement
        }
    }
}

impl SystemRequirements {
    /// Create a new system requirements checker with custom requirements
    pub fn new(
        required_commands: Vec<String>,
        required_libraries: Vec<String>,
        required_directories: Vec<String>,
        min_disk_space_mb: u64,
    ) -> Self {
        Self {
            required_commands,
            required_libraries,
            required_directories,
            min_disk_space_mb,
        }
    }

    /// Check if a command is available in PATH
    fn check_command(&self, command: &str) -> Result<()> {
        #[cfg(unix)]
        let status = Command::new("which")
            .arg(command)
            .status();

        #[cfg(windows)]
        let status = Command::new("where")
            .arg(command)
            .status();

        match status {
            Ok(exit_status) if exit_status.success() => Ok(()),
            _ => Err(ShellBeError::SystemRequirement(format!(
                "Required command '{}' not found in PATH", command
            ))),
        }
    }

    /// Check if a library is available
    #[cfg(unix)]
    fn check_library(&self, library: &str) -> Result<()> {
        // On Unix, we can use ldconfig to check for libraries
        let status = Command::new("ldconfig")
            .arg("-p")
            .output();

        match status {
            Ok(output) if output.status.success() => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                if output_str.contains(library) {
                    Ok(())
                } else {
                    Err(ShellBeError::SystemRequirement(format!(
                        "Required library '{}' not found", library
                    )))
                }
            },
            _ => Err(ShellBeError::SystemRequirement(format!(
                "Could not check for library '{}', ldconfig failed", library
            ))),
        }
    }

    /// Check if a library is available
    #[cfg(windows)]
    fn check_library(&self, library: &str) -> Result<()> {
        // On Windows, checking for libraries is more complex
        // This is a simplified approach
        let lib_paths = vec![
            std::env::var("PATH").unwrap_or_default(),
            std::env::var("SYSTEMROOT").unwrap_or_default() + "\\System32",
        ];

        for path in lib_paths {
            for entry in std::path::Path::new(&path).read_dir().ok().into_iter().flatten() {
                if let Ok(entry) = entry {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.to_lowercase() == format!("{}.dll", library).to_lowercase() {
                            return Ok(());
                        }
                    }
                }
            }
        }

        Err(ShellBeError::SystemRequirement(format!(
            "Required library '{}' not found", library
        )))
    }

    /// Check if a directory exists and is writable
    fn check_directory(&self, directory: &str) -> Result<()> {
        let path = std::path::Path::new(directory);

        if !path.exists() {
            return Err(ShellBeError::SystemRequirement(format!(
                "Required directory '{}' does not exist", directory
            )));
        }

        if !path.is_dir() {
            return Err(ShellBeError::SystemRequirement(format!(
                "'{}' is not a directory", directory
            )));
        }

        // Test if directory is writable by creating a temporary file
        let test_file = path.join(".shellbe_write_test");
        match std::fs::File::create(&test_file) {
            Ok(_) => {
                // Clean up test file
                let _ = std::fs::remove_file(&test_file);
                Ok(())
            },
            Err(_) => Err(ShellBeError::SystemRequirement(format!(
                "Directory '{}' is not writable", directory
            ))),
        }
    }

    /// Check available disk space
    fn check_disk_space(&self, path: &str) -> Result<u64> {
        let path = std::path::Path::new(path);

        #[cfg(unix)]
        {
            use std::mem::MaybeUninit;

            let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
                .map_err(|_| ShellBeError::SystemRequirement(
                    "Path contains invalid characters".to_string()
                ))?;

            unsafe {
                let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
                if libc::statvfs(path_cstr.as_ptr(), stat.as_mut_ptr()) == 0 {
                    let stat = stat.assume_init();
                    let available_kb = (stat.f_bavail as u64) * (stat.f_frsize as u64) / 1024;
                    let available_mb = available_kb / 1024;
                    return Ok(available_mb);
                }
            }

            Err(ShellBeError::SystemRequirement(
                "Could not determine available disk space".to_string()
            ))
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use std::ffi::OsStr;
            use std::ptr;
            use winapi::um::fileapi::{GetDiskFreeSpaceExW, PULARGE_INTEGER};

            let path: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();

            let mut free_bytes: winapi::shared::minwindef::ULARGE_INTEGER = unsafe { std::mem::zeroed() };
            let ret = unsafe {
                GetDiskFreeSpaceExW(
                    path.as_ptr(),
                    &mut free_bytes as PULARGE_INTEGER,
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };

            if ret != 0 {
                let available_bytes = unsafe { *free_bytes.QuadPart() };
                let available_mb = available_bytes / 1024 / 1024;
                return Ok(available_mb);
            }

            Err(ShellBeError::SystemRequirement(
                "Could not determine available disk space".to_string()
            ))
        }
    }

    /// Check all system requirements
    pub fn check_all(&self) -> HashMap<String, Result<()>> {
        let mut results = HashMap::new();

        // Check commands
        for command in &self.required_commands {
            results.insert(
                format!("command:{}", command),
                self.check_command(command),
            );
        }

        // Check libraries
        for library in &self.required_libraries {
            results.insert(
                format!("library:{}", library),
                self.check_library(library),
            );
        }

        // Check directories
        for directory in &self.required_directories {
            results.insert(
                format!("directory:{}", directory),
                self.check_directory(directory),
            );
        }

        // Check disk space for home directory
        if let Some(home_dir) = dirs::home_dir() {
            match self.check_disk_space(home_dir.to_str().unwrap_or("")) {
                Ok(available_mb) => {
                    if available_mb < self.min_disk_space_mb {
                        results.insert(
                            "disk_space".to_string(),
                            Err(ShellBeError::SystemRequirement(format!(
                                "Not enough disk space: {} MB available, {} MB required",
                                available_mb, self.min_disk_space_mb
                            ))),
                        );
                    } else {
                        results.insert("disk_space".to_string(), Ok(()));
                    }
                },
                Err(e) => {
                    results.insert("disk_space".to_string(), Err(e));
                },
            }
        }

        results
    }

    /// Check if all requirements are met
    pub fn all_requirements_met(&self) -> Result<()> {
        let results = self.check_all();

        let mut failed_checks = Vec::new();

        for (requirement, result) in results {
            if let Err(e) = result {
                failed_checks.push(format!("{}: {}", requirement, e));
            }
        }

        if failed_checks.is_empty() {
            Ok(())
        } else {
            Err(ShellBeError::SystemRequirement(format!(
                "System requirements not met:\n{}",
                failed_checks.join("\n")
            )))
        }
    }

    /// Add a required command
    pub fn add_required_command(&mut self, command: &str) {
        self.required_commands.push(command.to_string());
    }

    /// Add a required library
    pub fn add_required_library(&mut self, library: &str) {
        self.required_libraries.push(library.to_string());
    }

    /// Add a required directory
    pub fn add_required_directory(&mut self, directory: &str) {
        self.required_directories.push(directory.to_string());
    }

    /// Set minimum disk space requirement
    pub fn set_min_disk_space_mb(&mut self, min_disk_space_mb: u64) {
        self.min_disk_space_mb = min_disk_space_mb;
    }
}