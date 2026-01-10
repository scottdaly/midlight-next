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
            if !path.extension().is_some_and(|ext| ext == "json") {
                continue;
            }

            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
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

            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
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

    // ============================================
    // Basic WAL operations
    // ============================================

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
        manager
            .write_wal("folder/file2.md", "content2")
            .await
            .unwrap();

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

    // ============================================
    // Content change detection
    // ============================================

    #[tokio::test]
    async fn test_content_change_detection() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // First write succeeds
        let result = manager.write_wal("file.md", "content v1").await.unwrap();
        assert!(result);

        // Same content is skipped
        let result = manager.write_wal("file.md", "content v1").await.unwrap();
        assert!(!result);

        // Different content succeeds
        let result = manager.write_wal("file.md", "content v2").await.unwrap();
        assert!(result);

        // Same new content is skipped
        let result = manager.write_wal("file.md", "content v2").await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_content_hash_independence() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write to two different files with same content
        let result1 = manager.write_wal("file1.md", "same content").await.unwrap();
        let result2 = manager.write_wal("file2.md", "same content").await.unwrap();

        // Both should succeed (different file keys)
        assert!(result1);
        assert!(result2);
    }

    // ============================================
    // Recovery content retrieval
    // ============================================

    #[tokio::test]
    async fn test_get_recovery_content() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // No recovery initially
        let content = manager.get_recovery_content("file.md").await.unwrap();
        assert!(content.is_none());

        // Write WAL
        let test_content = r#"{"type":"doc","content":[{"type":"paragraph"}]}"#;
        manager.write_wal("file.md", test_content).await.unwrap();

        // Get recovery content
        let content = manager.get_recovery_content("file.md").await.unwrap();
        assert_eq!(content, Some(test_content.to_string()));
    }

    #[tokio::test]
    async fn test_has_unique_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // No recovery means not unique
        let result = manager
            .has_unique_recovery("file.md", "any content")
            .await
            .unwrap();
        assert!(!result);

        // Write recovery content
        manager.write_wal("file.md", "recovery content").await.unwrap();

        // Different content is unique
        let result = manager
            .has_unique_recovery("file.md", "different content")
            .await
            .unwrap();
        assert!(result);

        // Same content is not unique
        let result = manager
            .has_unique_recovery("file.md", "recovery content")
            .await
            .unwrap();
        assert!(!result);
    }

    // ============================================
    // Discard operations
    // ============================================

    #[tokio::test]
    async fn test_discard_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write and then discard
        manager.write_wal("file.md", "content").await.unwrap();
        assert!(manager.has_recovery("file.md").await);

        manager.discard_recovery("file.md").await.unwrap();
        assert!(!manager.has_recovery("file.md").await);
    }

    #[tokio::test]
    async fn test_discard_nonexistent_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Should not fail when discarding nonexistent recovery
        let result = manager.discard_recovery("nonexistent.md").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_discard_all_clears_state() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write WAL
        manager.write_wal("file.md", "content").await.unwrap();

        // Discard all
        manager.discard_all_recovery().await.unwrap();

        // Writing same content should succeed (state was cleared)
        let result = manager.write_wal("file.md", "content").await.unwrap();
        assert!(result, "State should be cleared after discard_all");
    }

    // ============================================
    // Large content handling
    // ============================================

    #[tokio::test]
    async fn test_large_content() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Create large content (1MB)
        let large_content = "x".repeat(1024 * 1024);

        // Write should succeed
        let result = manager.write_wal("large.md", &large_content).await.unwrap();
        assert!(result);

        // Read back should match
        let content = manager.get_recovery_content("large.md").await.unwrap();
        assert_eq!(content, Some(large_content));
    }

    #[tokio::test]
    async fn test_unicode_content() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        let unicode_content = "Hello 世界! Привет мир! 🎉 café naïve";

        manager.write_wal("unicode.md", unicode_content).await.unwrap();

        let content = manager.get_recovery_content("unicode.md").await.unwrap();
        assert_eq!(content, Some(unicode_content.to_string()));
    }

    // ============================================
    // Edge cases
    // ============================================

    #[tokio::test]
    async fn test_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Empty content should work
        manager.write_wal("empty.md", "").await.unwrap();

        let content = manager.get_recovery_content("empty.md").await.unwrap();
        assert_eq!(content, Some(String::new()));
    }

    #[tokio::test]
    async fn test_special_characters_in_file_key() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // File keys with special characters
        let file_keys = vec![
            "path/with spaces/file.md",
            "path/with-dashes/file.md",
            "path/with_underscores/file.md",
            "deeply/nested/path/to/file.md",
            "file with (parens).md",
        ];

        for key in &file_keys {
            manager.write_wal(key, "content").await.unwrap();
            assert!(manager.has_recovery(key).await, "Recovery should exist for: {}", key);
        }

        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), file_keys.len());
    }

    #[tokio::test]
    async fn test_check_recovery_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Empty recovery directory
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert!(recoverable.is_empty());
    }

    #[tokio::test]
    async fn test_check_recovery_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        // Don't call init() - directory doesn't exist

        // Should return empty, not error
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert!(recoverable.is_empty());
    }

    #[tokio::test]
    async fn test_discard_all_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        // Don't call init()

        // Should succeed even if directory doesn't exist
        let result = manager.discard_all_recovery().await;
        assert!(result.is_ok());
    }

    // ============================================
    // WAL file format
    // ============================================

    #[tokio::test]
    async fn test_wal_file_format() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        manager.write_wal("test.md", "test content").await.unwrap();

        // Read the WAL file directly to verify format
        let recovery_dir = temp_dir.path().join(".midlight").join("recovery");
        let mut entries = std::fs::read_dir(&recovery_dir).unwrap();
        let entry = entries.next().unwrap().unwrap();
        let content = std::fs::read_to_string(entry.path()).unwrap();

        // Parse as JSON to verify structure
        let wal: WalFile = serde_json::from_str(&content).unwrap();
        assert_eq!(wal.version, WAL_VERSION);
        assert_eq!(wal.file_key, "test.md");
        assert_eq!(wal.content, "test content");
        assert!(!wal.workspace_root.is_empty());
    }

    #[tokio::test]
    async fn test_wal_timestamp() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        let before = Utc::now();
        manager.write_wal("test.md", "content").await.unwrap();
        let after = Utc::now();

        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), 1);

        let wal_time = recoverable[0].wal_time;
        assert!(wal_time >= before && wal_time <= after);
    }

    // ============================================
    // Multiple files operations
    // ============================================

    #[tokio::test]
    async fn test_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write multiple files
        for i in 0..10 {
            let key = format!("file{}.md", i);
            let content = format!("content for file {}", i);
            manager.write_wal(&key, &content).await.unwrap();
        }

        // All should be recoverable
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), 10);

        // Clear some
        for i in 0..5 {
            let key = format!("file{}.md", i);
            manager.clear_wal(&key).await.unwrap();
        }

        // Only half remain
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), 5);
    }

    #[tokio::test]
    async fn test_concurrent_writes_sequential() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Sequential writes to different files
        for i in 0..20 {
            let key = format!("concurrent{}.md", i);
            manager.write_wal(&key, &format!("content {}", i)).await.unwrap();
        }

        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), 20);
    }

    // ============================================
    // Corrupted file handling
    // ============================================

    #[tokio::test]
    async fn test_corrupted_wal_file() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write valid WAL
        manager.write_wal("valid.md", "valid content").await.unwrap();

        // Create corrupted WAL file
        let recovery_dir = temp_dir.path().join(".midlight").join("recovery");
        std::fs::write(
            recovery_dir.join("corrupted.wal.json"),
            "not valid json {{{",
        ).unwrap();

        // check_for_recovery should skip corrupted file but return valid ones
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), 1);
        assert_eq!(recoverable[0].file_key, "valid.md");
    }

    #[tokio::test]
    async fn test_non_wal_files_ignored() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write valid WAL
        manager.write_wal("test.md", "content").await.unwrap();

        // Create non-WAL files in recovery directory
        let recovery_dir = temp_dir.path().join(".midlight").join("recovery");
        std::fs::write(recovery_dir.join("random.txt"), "not a wal").unwrap();
        std::fs::write(recovery_dir.join("data.json"), "{}").unwrap();

        // Only the actual WAL file should be found
        let recoverable = manager.check_for_recovery().await.unwrap();
        assert_eq!(recoverable.len(), 1);
    }

    // ============================================
    // Clear after write cycles
    // ============================================

    #[tokio::test]
    async fn test_write_clear_write_cycle() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf());
        manager.init().await.unwrap();

        // Write, clear, write cycle
        manager.write_wal("file.md", "content v1").await.unwrap();
        manager.clear_wal("file.md").await.unwrap();

        // After clear, writing same content should succeed
        let result = manager.write_wal("file.md", "content v1").await.unwrap();
        assert!(result, "Write after clear should succeed");

        // And it should be recoverable
        assert!(manager.has_recovery("file.md").await);
    }
}
