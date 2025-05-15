use crate::domain::{HistoryRepository, HistoryEntry, DomainError};
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
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)
                .map_err(|e| DomainError::IoError(e))?;
        }

        let history_path = config_dir.join(&history_file);
        let history: Vec<HistoryEntry> = if history_path.exists() {
            let file = fs::File::open(&history_path)
                .map_err(|e| DomainError::IoError(e))?;

            serde_json::from_reader(file)
                .map_err(|e| DomainError::ConfigError(format!("Failed to parse history: {}", e)))?
        } else {
            Vec::new()
        };

        Ok(Self {
            config_dir,
            history_file,
            history: Arc::new(RwLock::new(history)),
        })
    }

    /// Save history to disk
    async fn save_history(&self) -> Result<(), DomainError> {
        let history_path = self.config_dir.join(&self.history_file);
        let history = self.history.read().await;

        let file = fs::File::create(&history_path)
            .map_err(|e| DomainError::IoError(e))?;

        serde_json::to_writer_pretty(file, &*history)
            .map_err(|e| DomainError::ConfigError(format!("Failed to save history: {}", e)))?;

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