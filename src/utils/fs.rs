use std::path::{Path, PathBuf};
use std::io;
use tokio::fs;

/// Ensure a directory exists with proper permissions
pub async fn ensure_directory(path: &Path) -> io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path).await?;

        // Set proper permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(path, permissions).await?;
        }
    }

    Ok(())
}

/// Ensure a file exists with proper permissions
pub async fn ensure_file(path: &Path, default_content: Option<&str>) -> io::Result<()> {
    if !path.exists() {
        // Make sure parent directory exists
        if let Some(parent) = path.parent() {
            ensure_directory(parent).await?;
        }

        // Create file with default content
        if let Some(content) = default_content {
            fs::write(path, content).await?;
        } else {
            fs::write(path, "").await?;
        }

        // Set proper permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(path, permissions).await?;
        }
    }

    Ok(())
}

/// Create a backup of a file with timestamp
pub async fn backup_file(path: &Path) -> io::Result<PathBuf> {
    if !path.exists() {
        return Ok(path.to_owned());
    }

    let backup_path = path.with_extension(
        format!("backup.{}", chrono::Utc::now().format("%Y%m%d%H%M%S"))
    );

    fs::copy(path, &backup_path).await?;

    Ok(backup_path)
}

/// Get the shellbe config directory, creating it if it doesn't exist
pub async fn shellbe_config_dir() -> io::Result<PathBuf> {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".shellbe");

    ensure_directory(&dir).await?;

    Ok(dir)
}

/// Get the SSH config directory, creating it if it doesn't exist
pub async fn ssh_config_dir() -> io::Result<PathBuf> {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh");

    ensure_directory(&dir).await?;

    Ok(dir)
}