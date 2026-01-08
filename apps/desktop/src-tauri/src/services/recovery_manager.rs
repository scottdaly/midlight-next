// Recovery Manager - Write-Ahead Log (WAL) based crash recovery
//
// Maintains recovery files for open documents. If the app crashes,
// unsaved work can be recovered on next startup.
//
// WAL files are stored at: .midlight/recovery/{hash}.wal.json
// Format:
// {
//   "version": 1,
//   "file_key": "notes/ideas.md",
//   "content": "{\"type\":\"doc\",...}",
//   "timestamp": "2025-01-08T12:34:56Z",
//   "workspace_root": "/Users/..."
// }

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::fs;
use tracing::{debug, info, warn};
use xxhash_rust::xxh64::xxh64;

// ============================================================================
// Types
// ============================================================================

const WAL_VERSION: u32 = 1;

/// WAL file format stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalFile {
    pub version: u32,
    pub file_key: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub workspace_root: String,
}

/// Recovery file info returned to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryFile {
    pub file_key: String,
    pub wal_content: String,
    pub wal_time: DateTime<Utc>,
    pub workspace_root: String,
}

/// State for tracking active files
#[derive(Debug)]
struct FileState {
    last_content_hash: u64,
}

/// Recovery Manager maintains WAL files for crash recovery
pub struct RecoveryManager {
    workspace_root: PathBuf,
    recovery_dir: PathBuf,
    /// Track content hashes to avoid redundant writes
    file_states: Mutex<HashMap<String, FileState>>,
}

impl RecoveryManager {
    /// Create a new RecoveryManager for the given workspace
    pub fn new(workspace_root: PathBuf) -> Self {
        let recovery_dir = workspace_root.join(".midlight").join("recovery");
        Self {
            workspace_root,
            recovery_dir,
            file_states: Mutex::new(HashMap::new()),
        }
    }

    /// Initialize the recovery directory
    pub async fn init(&self) -> Result<(), String> {
        fs::create_dir_all(&self.recovery_dir)
            .await
            .map_err(|e| format!("Failed to create recovery directory: {}", e))?;

        debug!("Recovery manager initialized at {:?}", self.recovery_dir);
        Ok(())
    }

    /// Write WAL file for a document
    /// Returns true if content was written (changed), false if skipped (unchanged)
    pub async fn write_wal(&self, file_key: &str, content: &str) -> Result<bool, String> {
        let content_hash = xxh64(content.as_bytes(), 0);

        // Check if content has changed
        {
            let states = self.file_states.lock().unwrap();
            if let Some(state) = states.get(file_key) {
                if state.last_content_hash == content_hash {
                    debug!("WAL skipped for {} (unchanged)", file_key);
                    return Ok(false);
                }
            }
        }

        // Build WAL file
        let wal = WalFile {
            version: WAL_VERSION,
            file_key: file_key.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            workspace_root: self.workspace_root.to_string_lossy().to_string(),
        };

        let wal_path = self.get_wal_path(file_key);
        let wal_json = serde_json::to_string_pretty(&wal)
            .map_err(|e| format!("Failed to serialize WAL: {}", e))?;

        // Write atomically (write to temp, then rename)
        let temp_path = wal_path.with_extension("wal.tmp");
        fs::write(&temp_path, &wal_json)
            .await
            .map_err(|e| format!("Failed to write WAL temp file: {}", e))?;

        fs::rename(&temp_path, &wal_path)
            .await
            .map_err(|e| format!("Failed to rename WAL file: {}", e))?;

        // Update state
        {
            let mut states = self.file_states.lock().unwrap();
            states.insert(
                file_key.to_string(),
                FileState {
                    last_content_hash: content_hash,
                },
            );
        }

        debug!("WAL written for {}", file_key);
        Ok(true)
    }

    /// Clear WAL file after successful save
    pub async fn clear_wal(&self, file_key: &str) -> Result<(), String> {
        let wal_path = self.get_wal_path(file_key);

        // Remove from state tracking
        {
            let mut states = self.file_states.lock().unwrap();
            states.remove(file_key);
        }

        // Delete the WAL file if it exists
        if wal_path.exists() {
            fs::remove_file(&wal_path)
                .await
                .map_err(|e| format!("Failed to remove WAL file: {}", e))?;
            debug!("WAL cleared for {}", file_key);
        }

        Ok(())
    }

    /// Check for recovery files on startup
    /// Returns list of files with unsaved changes
    pub async fn check_for_recovery(&self) -> Result<Vec<RecoveryFile>, String> {
        let mut recoverable = Vec::new();

        // Ensure recovery directory exists
        if !self.recovery_dir.exists() {
            return Ok(recoverable);
        }

        let mut entries = fs::read_dir(&self.recovery_dir)
            .await
            .map_err(|e| format!("Failed to read recovery directory: {}", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read directory entry: {}", e))?
        {
            let path = entry.path();

            // Only process .wal.json files
            if !path.extension().map_or(false, |ext| ext == "json") {
                continue;
            }

            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !name.ends_with(".wal.json") {
                continue;
            }

            match self.read_wal_file(&path).await {
                Ok(wal) => {
                    info!("Found recovery file for: {}", wal.file_key);
                    recoverable.push(RecoveryFile {
                        file_key: wal.file_key,
                        wal_content: wal.content,
                        wal_time: wal.timestamp,
                        workspace_root: wal.workspace_root,
                    });
                }
                Err(e) => {
                    warn!("Failed to read recovery file {:?}: {}", path, e);
                }
            }
        }

        Ok(recoverable)
    }

    /// Check if a specific file has recovery available
    pub async fn has_recovery(&self, file_key: &str) -> bool {
        let wal_path = self.get_wal_path(file_key);
        wal_path.exists()
    }

    /// Get recovery content for a specific file
    pub async fn get_recovery_content(&self, file_key: &str) -> Result<Option<String>, String> {
        let wal_path = self.get_wal_path(file_key);

        if !wal_path.exists() {
            return Ok(None);
        }

        let wal = self.read_wal_file(&wal_path).await?;
        Ok(Some(wal.content))
    }

    /// Discard recovery for a specific file (user chose not to recover)
    pub async fn discard_recovery(&self, file_key: &str) -> Result<(), String> {
        self.clear_wal(file_key).await
    }

    /// Discard all recovery files
    pub async fn discard_all_recovery(&self) -> Result<(), String> {
        if !self.recovery_dir.exists() {
            return Ok(());
        }

        let mut entries = fs::read_dir(&self.recovery_dir)
            .await
            .map_err(|e| format!("Failed to read recovery directory: {}", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read directory entry: {}", e))?
        {
            let path = entry.path();

            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if name.ends_with(".wal.json") || name.ends_with(".wal.tmp") {
                if let Err(e) = fs::remove_file(&path).await {
                    warn!("Failed to remove recovery file {:?}: {}", path, e);
                }
            }
        }

        // Clear all tracked states
        {
            let mut states = self.file_states.lock().unwrap();
            states.clear();
        }

        info!("All recovery files discarded");
        Ok(())
    }

    /// Compare recovery content with current file content
    /// Returns true if recovery has different content
    pub async fn has_unique_recovery(
        &self,
        file_key: &str,
        current_content: &str,
    ) -> Result<bool, String> {
        let recovery_content = self.get_recovery_content(file_key).await?;

        match recovery_content {
            Some(content) => Ok(content != current_content),
            None => Ok(false),
        }
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    fn get_wal_path(&self, file_key: &str) -> PathBuf {
        // Use hash of file_key as filename for safe filesystem names
        let hash = xxh64(file_key.as_bytes(), 0);
        self.recovery_dir.join(format!("{:016x}.wal.json", hash))
    }

    async fn read_wal_file(&self, path: &PathBuf) -> Result<WalFile, String> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read WAL file: {}", e))?;

        let wal: WalFile = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse WAL file: {}", e))?;

        // Version check for future compatibility
        if wal.version > WAL_VERSION {
            warn!(
                "WAL file version {} is newer than supported version {}",
                wal.version, WAL_VERSION
            );
        }

        Ok(wal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_and_clear_wal() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write WAL
        let result = manager
            .write_wal("test/file.md", r#"{"type":"doc","content":[]}"#)
            .await
            .unwrap();
        assert!(result, "First write should succeed");

        // Check recovery exists
        assert!(manager.has_recovery("test/file.md").await);

        // Write same content should be skipped
        let result = manager
            .write_wal("test/file.md", r#"{"type":"doc","content":[]}"#)
            .await
            .unwrap();
        assert!(!result, "Same content should be skipped");

        // Clear WAL
        manager.clear_wal("test/file.md").await.unwrap();
        assert!(!manager.has_recovery("test/file.md").await);
    }

    #[tokio::test]
    async fn test_check_for_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write some WAL files
        manager.write_wal("file1.md", "content1").await.unwrap();
        manager.write_wal("folder/file2.md", "content2").await.unwrap();

        // Check for recovery
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), 2);

        // Verify file keys are present
        let keys: Vec<_> = recoverable.iter().map(|r| r.file_key.as_str()).collect();
        assert!(keys.contains(&"file1.md"));
        assert!(keys.contains(&"folder/file2.md"));
    }

    #[tokio::test]
    async fn test_discard_all_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write some WAL files
        manager.write_wal("file1.md", "content1").await.unwrap();
        manager.write_wal("file2.md", "content2").await.unwrap();

        // Discard all
        manager.discard_all_recovery().await.unwrap();

        // Check nothing remains
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert!(recoverable.is_empty());
    }
}
