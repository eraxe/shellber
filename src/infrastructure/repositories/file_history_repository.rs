use crate::domain::{HistoryRepository, HistoryEntry, DomainError};
use crate::utils::{FileLock, ensure_directory, ensure_file};
use async_trait::async_trait;
use std::path::PathBuf;
use std::fs;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// File-based implementation of the history repository
pub struct FileHistoryRepository {
    config_dir: PathBuf,
    history_file: String,
    history: Arc<RwLock<Vec<HistoryEntry>>>,
}

impl FileHistoryRepository {
    /// Create a new file-based history repository
    pub async fn new(config_dir: PathBuf, history_file: String) -> Result<Self, DomainError> {
        // Create config directory if it doesn't exist
        ensure_directory(&config_dir).await
            .map_err(|e| DomainError::IoError(e))?;

        let history_path = config_dir.join(&history_file);
        let history: Vec<HistoryEntry> = if history_path.exists() {
            let file = fs::File::open(&history_path)
                .map_err(|e| DomainError::IoError(e))?;

            serde_json::from_reader(file)
                .map_err(|e| DomainError::ConfigError(format!("Failed to parse history: {}", e)))?
        } else {
            // Create an empty history file
            ensure_file(&history_path, Some("[]")).await
                .map_err(|e| DomainError::IoError(e))?;
            Vec::new()
        };

        Ok(Self {
            config_dir,
            history_file,
            history: Arc::new(RwLock::new(history)),
        })
    }

    /// Save history to disk with proper file locking
    async fn save_history(&self) -> Result<(), DomainError> {
        let history_path = self.config_dir.join(&self.history_file);

        // Acquire a lock for writing
        let mut lock = FileLock::new(&history_path).await;
        if !lock.acquire(5000).await.map_err(|e| DomainError::IoError(e))? {
            return Err(DomainError::ConfigError("Failed to acquire lock for writing history".to_string()));
        }

        // Get a snapshot of the history
        let history = {
            let history = self.history.read().await;
            history.clone()
        };

        // Write to a temporary file first
        let temp_path = history_path.with_extension("temp");
        let file = fs::File::create(&temp_path)
            .map_err(|e| DomainError::IoError(e))?;

        serde_json::to_writer_pretty(file, &history)
            .map_err(|e| DomainError::ConfigError(format!("Failed to save history: {}", e)))?;

        // Rename the temporary file to the actual file
        // This provides atomic file replacement
        fs::rename(&temp_path, &history_path)
            .map_err(|e| DomainError::IoError(e))?;

        // Release the lock
        lock.release().await.map_err(|e| DomainError::IoError(e))?;

        Ok(())
    }
}

#[async_trait]
impl HistoryRepository for FileHistoryRepository {
    /// Add a history entry
    async fn add(&self, entry: HistoryEntry) -> Result<(), DomainError> {
        let mut history = self.history.write().await;
        history.push(entry);
        drop(history);

        self.save_history().await
    }

    /// Get recent history entries
    async fn get_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>, DomainError> {
        let history = self.history.read().await;

        // Return the most recent entries up to the limit
        let start = if history.len() > limit {
            history.len() - limit
        } else {
            0
        };

        Ok(history[start..].to_vec())
    }

    /// Get history for a specific profile
    async fn get_for_profile(&self, profile_name: &str) -> Result<Vec<HistoryEntry>, DomainError> {
        let history = self.history.read().await;

        let result = history.iter()
            .filter(|entry| entry.profile_name == profile_name)
            .cloned()
            .collect();

        Ok(result)
    }

    /// Get connection statistics
    async fn get_stats(&self) -> Result<HashMap<String, usize>, DomainError> {
        let history = self.history.read().await;
        let mut stats = HashMap::new();

        for entry in history.iter() {
            *stats.entry(entry.profile_name.clone()).or_insert(0) += 1;
        }

        Ok(stats)
    }
}