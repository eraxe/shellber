use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io;
use std::time::Duration;
use tokio::time::sleep;

pub struct FileLock {
    lock_file: PathBuf,
    _file_handle: Option<File>,
}

impl FileLock {
    pub async fn new(path: &Path) -> Self {
        let lock_file = path.with_extension("lock");
        Self {
            lock_file,
            _file_handle: None,
        }
    }

    pub async fn acquire(&mut self, timeout_ms: u64) -> io::Result<bool> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&self.lock_file)
            {
                Ok(file) => {
                    self._file_handle = Some(file);
                    return Ok(true);
                },
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    // Check if the lock file is stale (older than 30 seconds)
                    if let Ok(metadata) = self.lock_file.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if std::time::SystemTime::now().duration_since(modified).unwrap_or_default() > Duration::from_secs(30) {
                                // Stale lock, try to remove it
                                if std::fs::remove_file(&self.lock_file).is_ok() {
                                    continue;
                                }
                            }
                        }
                    }

                    // If we've timed out, return false
                    if start.elapsed() > timeout {
                        return Ok(false);
                    }

                    // Wait a bit before trying again
                    sleep(Duration::from_millis(100)).await;
                },
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn release(&mut self) -> io::Result<()> {
        self._file_handle = None;
        if self.lock_file.exists() {
            tokio::fs::remove_file(&self.lock_file).await?;
        }
        Ok(())
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        self._file_handle = None;
        if self.lock_file.exists() {
            let _ = std::fs::remove_file(&self.lock_file);
        }
    }
}