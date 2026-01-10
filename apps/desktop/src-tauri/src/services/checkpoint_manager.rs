// Checkpoint manager - Version history with retention policies

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::error::{MidlightError, Result};
use super::object_store::ObjectStore;
use crate::traits::{ObjectStoreOps, RealTimeProvider, TimeProvider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    #[serde(rename = "contentHash")]
    pub content_hash: String,
    #[serde(rename = "sidecarHash")]
    pub sidecar_hash: String,
    pub timestamp: String,
    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,
    #[serde(rename = "type")]
    pub checkpoint_type: String, // "auto" | "bookmark"
    pub label: Option<String>,
    pub description: Option<String>,
    pub stats: CheckpointStats,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointStats {
    #[serde(rename = "wordCount")]
    pub word_count: u32,
    #[serde(rename = "charCount")]
    pub char_count: u32,
    #[serde(rename = "changeSize")]
    pub change_size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointHistory {
    #[serde(rename = "fileKey")]
    pub file_key: String,
    #[serde(rename = "headId")]
    pub head_id: Option<String>,
    pub checkpoints: Vec<Checkpoint>,
}

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub min_interval_seconds: u64,
    pub min_change_threshold: u32,
    pub max_checkpoints_per_file: usize,
    pub retention_days: u64,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            min_interval_seconds: 300, // 5 minutes
            min_change_threshold: 50,  // 50 characters
            max_checkpoints_per_file: 50,
            retention_days: 7,
        }
    }
}

/// Manages version history for documents
pub struct CheckpointManager<O: ObjectStoreOps = ObjectStore, T: TimeProvider = RealTimeProvider> {
    checkpoints_dir: PathBuf,
    object_store: Arc<O>,
    time_provider: Arc<T>,
    config: CheckpointConfig,
    histories: HashMap<String, CheckpointHistory>,
}

impl CheckpointManager<ObjectStore, RealTimeProvider> {
    /// Create a new CheckpointManager with default implementations
    pub fn new(workspace_root: &Path, object_store: ObjectStore) -> Self {
        Self {
            checkpoints_dir: workspace_root.join(".midlight").join("checkpoints"),
            object_store: Arc::new(object_store),
            time_provider: Arc::new(RealTimeProvider::new()),
            config: CheckpointConfig::default(),
            histories: HashMap::new(),
        }
    }
}

impl<O: ObjectStoreOps, T: TimeProvider> CheckpointManager<O, T> {
    /// Create a new CheckpointManager with custom dependencies (for testing)
    #[allow(dead_code)]
    pub fn with_deps(workspace_root: &Path, object_store: Arc<O>, time_provider: Arc<T>) -> Self {
        Self {
            checkpoints_dir: workspace_root.join(".midlight").join("checkpoints"),
            object_store,
            time_provider,
            config: CheckpointConfig::default(),
            histories: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_config(mut self, config: CheckpointConfig) -> Self {
        self.config = config;
        self
    }

    /// Get the config (for testing)
    #[cfg(test)]
    pub fn config(&self) -> &CheckpointConfig {
        &self.config
    }

    /// Initialize the checkpoints directory
    pub async fn init(&self) -> Result<()> {
        std::fs::create_dir_all(&self.checkpoints_dir)?;
        Ok(())
    }

    /// Convert file path to a safe key for storage
    fn path_to_key(path: &str) -> String {
        path.replace(['/', '\\'], "__").replace('.', "_")
    }

    /// Get the history file path for a document
    fn get_history_path(&self, file_path: &str) -> PathBuf {
        let key = Self::path_to_key(file_path);
        self.checkpoints_dir.join(format!("{}.json", key))
    }

    /// Load checkpoint history for a file
    pub async fn load_history(&mut self, file_path: &str) -> Result<&CheckpointHistory> {
        let key = Self::path_to_key(file_path);

        if !self.histories.contains_key(&key) {
            let history_path = self.get_history_path(file_path);

            let history = if history_path.exists() {
                let content = std::fs::read_to_string(&history_path)?;
                serde_json::from_str(&content)?
            } else {
                CheckpointHistory {
                    file_key: key.clone(),
                    head_id: None,
                    checkpoints: vec![],
                }
            };

            self.histories.insert(key.clone(), history);
        }

        Ok(self.histories.get(&key).unwrap())
    }

    /// Save checkpoint history for a file
    async fn save_history(&self, file_path: &str, history: &CheckpointHistory) -> Result<()> {
        let history_path = self.get_history_path(file_path);
        let content = serde_json::to_string_pretty(history)?;
        std::fs::write(history_path, content)?;
        Ok(())
    }

    /// Create a new checkpoint
    pub async fn create_checkpoint(
        &mut self,
        file_path: &str,
        markdown: &str,
        sidecar: &str,
        trigger: &str,
        label: Option<&str>,
        description: Option<&str>,
    ) -> Result<Checkpoint> {
        // Store content in object store
        let content_hash = self.object_store.write(markdown).await?;
        let sidecar_hash = self.object_store.write(sidecar).await?;

        let key = Self::path_to_key(file_path);

        // Load existing history
        self.load_history(file_path).await?;

        // Take the history out temporarily to avoid borrow conflicts
        let mut history = self.histories.remove(&key).unwrap();

        // Check if we should create a checkpoint
        let now = self.time_provider.now_utc();
        if trigger != "bookmark"
            && !Self::should_create_checkpoint(&self.config, &history, markdown, now)
        {
            // Return the head checkpoint if exists
            if let Some(head_id) = &history.head_id {
                if let Some(cp) = history.checkpoints.iter().find(|c| &c.id == head_id) {
                    let result = cp.clone();
                    // Put history back
                    self.histories.insert(key, history);
                    return Ok(result);
                }
            }
        }

        // Generate checkpoint ID
        let id = format!("cp-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let timestamp = now.to_rfc3339();

        // Calculate stats
        let word_count = markdown.split_whitespace().count() as u32;
        let char_count = markdown.len() as u32;
        let change_size = if let Some(head_id) = &history.head_id {
            if let Some(prev) = history.checkpoints.iter().find(|c| &c.id == head_id) {
                (char_count as i32) - (prev.stats.char_count as i32)
            } else {
                char_count as i32
            }
        } else {
            char_count as i32
        };

        let checkpoint = Checkpoint {
            id: id.clone(),
            content_hash,
            sidecar_hash,
            timestamp,
            parent_id: history.head_id.clone(),
            checkpoint_type: if label.is_some() { "bookmark" } else { "auto" }.to_string(),
            label: label.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
            stats: CheckpointStats {
                word_count,
                char_count,
                change_size,
            },
            trigger: trigger.to_string(),
        };

        // Add to history
        history.checkpoints.push(checkpoint.clone());
        history.head_id = Some(id);

        // Apply retention policy
        Self::apply_retention_policy(&self.config, &mut history, now);

        // Save history
        self.save_history(file_path, &history).await?;

        // Put history back
        self.histories.insert(key, history);

        tracing::debug!(
            "Created checkpoint {} for {} ({})",
            &checkpoint.id[..8],
            file_path,
            trigger
        );

        Ok(checkpoint)
    }

    /// Check if we should create a new checkpoint based on config
    fn should_create_checkpoint(
        config: &CheckpointConfig,
        history: &CheckpointHistory,
        markdown: &str,
        now: DateTime<Utc>,
    ) -> bool {
        if history.checkpoints.is_empty() {
            return true;
        }

        // Check time since last checkpoint
        if let Some(head_id) = &history.head_id {
            if let Some(last) = history.checkpoints.iter().find(|c| &c.id == head_id) {
                if let Ok(last_time) = DateTime::parse_from_rfc3339(&last.timestamp) {
                    let elapsed = now.signed_duration_since(last_time.with_timezone(&Utc));
                    if elapsed < Duration::seconds(config.min_interval_seconds as i64) {
                        return false;
                    }
                }

                // Check change size
                let change_size =
                    (markdown.len() as i32 - last.stats.char_count as i32).unsigned_abs();
                if change_size < config.min_change_threshold {
                    return false;
                }
            }
        }

        true
    }

    /// Apply retention policy to checkpoint history
    fn apply_retention_policy(
        config: &CheckpointConfig,
        history: &mut CheckpointHistory,
        now: DateTime<Utc>,
    ) {
        let retention_cutoff = now - Duration::days(config.retention_days as i64);

        // Keep bookmarks, remove old auto checkpoints
        history.checkpoints.retain(|cp| {
            if cp.checkpoint_type == "bookmark" {
                return true;
            }

            if let Ok(cp_time) = DateTime::parse_from_rfc3339(&cp.timestamp) {
                return cp_time.with_timezone(&Utc) > retention_cutoff;
            }

            true
        });

        // Limit total checkpoints
        if history.checkpoints.len() > config.max_checkpoints_per_file {
            // Keep newest checkpoints, but always keep bookmarks
            let mut to_keep: Vec<_> = history
                .checkpoints
                .iter()
                .enumerate()
                .filter(|(_, cp)| cp.checkpoint_type == "bookmark")
                .map(|(i, _)| i)
                .collect();

            let auto_checkpoints: Vec<_> = history
                .checkpoints
                .iter()
                .enumerate()
                .filter(|(_, cp)| cp.checkpoint_type != "bookmark")
                .map(|(i, _)| i)
                .collect();

            // Add newest auto checkpoints until we hit the limit
            let slots_remaining = config
                .max_checkpoints_per_file
                .saturating_sub(to_keep.len());
            to_keep.extend(auto_checkpoints.iter().rev().take(slots_remaining));
            to_keep.sort();

            history.checkpoints = to_keep
                .iter()
                .filter_map(|&i| history.checkpoints.get(i).cloned())
                .collect();
        }
    }

    /// Get all checkpoints for a file
    pub async fn get_checkpoints(&mut self, file_path: &str) -> Result<Vec<Checkpoint>> {
        self.load_history(file_path).await?;
        let key = Self::path_to_key(file_path);

        Ok(self
            .histories
            .get(&key)
            .map(|h| h.checkpoints.clone())
            .unwrap_or_default())
    }

    /// Get a specific checkpoint
    pub async fn get_checkpoint(
        &mut self,
        file_path: &str,
        checkpoint_id: &str,
    ) -> Result<Checkpoint> {
        let checkpoints = self.get_checkpoints(file_path).await?;

        checkpoints
            .into_iter()
            .find(|cp| cp.id == checkpoint_id)
            .ok_or_else(|| MidlightError::CheckpointNotFound(checkpoint_id.to_string()))
    }

    /// Get content for a checkpoint
    pub async fn get_checkpoint_content(
        &self,
        checkpoint: &Checkpoint,
    ) -> Result<(String, String)> {
        let markdown = self.object_store.read(&checkpoint.content_hash).await?;
        let sidecar = self.object_store.read(&checkpoint.sidecar_hash).await?;
        Ok((markdown, sidecar))
    }

    /// Compare two checkpoints and return a diff
    pub async fn compare_checkpoints(
        &self,
        checkpoint_a: &Checkpoint,
        checkpoint_b: &Checkpoint,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let content_a = self.object_store.read(&checkpoint_a.content_hash).await?;
        let content_b = self.object_store.read(&checkpoint_b.content_hash).await?;

        let lines_a: Vec<&str> = content_a.lines().collect();
        let lines_b: Vec<&str> = content_b.lines().collect();

        // Simple line-by-line diff
        let mut additions = Vec::new();
        let mut deletions = Vec::new();

        for line in &lines_b {
            if !lines_a.contains(line) {
                additions.push((*line).to_string());
            }
        }

        for line in &lines_a {
            if !lines_b.contains(line) {
                deletions.push((*line).to_string());
            }
        }

        Ok((additions, deletions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::object_store::MockObjectStore;
    use crate::traits::time::MockTimeProvider;
    use tempfile::tempdir;

    fn create_test_manager() -> (
        tempfile::TempDir,
        CheckpointManager<MockObjectStore, MockTimeProvider>,
    ) {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200)); // 2024-01-01

        let manager = CheckpointManager::with_deps(temp.path(), object_store, time_provider);

        (temp, manager)
    }

    #[tokio::test]
    async fn test_create_checkpoint() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let checkpoint = manager
            .create_checkpoint("test.md", "Hello World", "{}", "manual", None, None)
            .await
            .unwrap();

        assert!(checkpoint.id.starts_with("cp-"));
        assert_eq!(checkpoint.checkpoint_type, "auto");
        assert_eq!(checkpoint.trigger, "manual");
        assert_eq!(checkpoint.stats.word_count, 2);
    }

    #[tokio::test]
    async fn test_create_bookmark() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let checkpoint = manager
            .create_checkpoint(
                "test.md",
                "# Hello World",
                "{}",
                "bookmark",
                Some("My Bookmark"),
                Some("A description"),
            )
            .await
            .unwrap();

        assert_eq!(checkpoint.checkpoint_type, "bookmark");
        assert_eq!(checkpoint.label, Some("My Bookmark".to_string()));
        assert_eq!(checkpoint.description, Some("A description".to_string()));
    }

    #[tokio::test]
    async fn test_get_checkpoints() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        // Create a few checkpoints
        manager
            .create_checkpoint("test.md", "Content 1", "{}", "manual", None, None)
            .await
            .unwrap();

        // Advance time to ensure new checkpoint is created
        // Access the time provider and advance it
        let time_provider = Arc::clone(&manager.time_provider);
        time_provider.advance_secs(400); // More than min_interval_seconds

        // Need content change >= 50 chars to trigger new checkpoint
        let long_content = "Content 2 with a lot more text that exceeds the minimum change threshold of 50 characters";
        manager
            .create_checkpoint("test.md", long_content, "{}", "manual", None, None)
            .await
            .unwrap();

        let checkpoints = manager.get_checkpoints("test.md").await.unwrap();
        assert_eq!(checkpoints.len(), 2);
    }

    #[tokio::test]
    async fn test_min_interval_enforcement() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        // Create first checkpoint
        let cp1 = manager
            .create_checkpoint("test.md", "Content 1", "{}", "manual", None, None)
            .await
            .unwrap();

        // Try to create another immediately (should return existing)
        let cp2 = manager
            .create_checkpoint(
                "test.md",
                "Content 2 with changes",
                "{}",
                "manual",
                None,
                None,
            )
            .await
            .unwrap();

        // Should be the same checkpoint since not enough time passed
        assert_eq!(cp1.id, cp2.id);
    }

    #[tokio::test]
    async fn test_min_change_threshold() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone());
        manager.init().await.unwrap();

        // Create first checkpoint
        let cp1 = manager
            .create_checkpoint("test.md", "Hello World", "{}", "manual", None, None)
            .await
            .unwrap();

        // Advance time past interval
        time_provider.advance_secs(400);

        // Try to create another with small change (should return existing)
        let cp2 = manager
            .create_checkpoint("test.md", "Hello World!", "{}", "manual", None, None)
            .await
            .unwrap();

        // Should be the same checkpoint since change is too small
        assert_eq!(cp1.id, cp2.id);
    }

    #[tokio::test]
    async fn test_retention_policy_age() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        // Start 10 days ago
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200 - 10 * 86400));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone());
        manager.init().await.unwrap();

        // Create old checkpoint
        manager
            .create_checkpoint("test.md", "Old content", "{}", "manual", None, None)
            .await
            .unwrap();

        // Advance time by 10 days (past retention period)
        time_provider.advance_days(10);

        // Create new checkpoint with significant changes
        manager
            .create_checkpoint(
                "test.md",
                "New content with lots of additional text to exceed threshold",
                "{}",
                "manual",
                None,
                None,
            )
            .await
            .unwrap();

        let checkpoints = manager.get_checkpoints("test.md").await.unwrap();

        // Old checkpoint should be removed due to retention policy
        assert_eq!(checkpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_bookmarks_preserved_by_retention() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200 - 10 * 86400));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone());
        manager.init().await.unwrap();

        // Create old bookmark - bookmarks bypass interval/threshold checks
        manager
            .create_checkpoint(
                "test.md",
                "Old content for bookmark that has enough text",
                "{}",
                "bookmark",
                Some("Important"),
                None,
            )
            .await
            .unwrap();

        // Advance time by 10 days
        time_provider.advance_days(10);

        // Create new checkpoint with content that exceeds 50 char change threshold
        let new_content = "New content that is significantly different from the old content and exceeds the minimum threshold";
        manager
            .create_checkpoint("test.md", new_content, "{}", "manual", None, None)
            .await
            .unwrap();

        let checkpoints = manager.get_checkpoints("test.md").await.unwrap();

        // Both should exist - bookmark is preserved even though it's older than retention
        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints
            .iter()
            .any(|cp| cp.checkpoint_type == "bookmark"));
    }

    #[tokio::test]
    async fn test_get_checkpoint_content() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let checkpoint = manager
            .create_checkpoint(
                "test.md",
                "# Hello",
                "{\"key\": \"value\"}",
                "manual",
                None,
                None,
            )
            .await
            .unwrap();

        let (markdown, sidecar) = manager.get_checkpoint_content(&checkpoint).await.unwrap();

        assert_eq!(markdown, "# Hello");
        assert_eq!(sidecar, "{\"key\": \"value\"}");
    }

    #[tokio::test]
    async fn test_compare_checkpoints() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone());
        manager.init().await.unwrap();

        // First checkpoint
        let content1 = "Line 1\nLine 2\nLine 3";
        let cp1 = manager
            .create_checkpoint("test.md", content1, "{}", "manual", None, None)
            .await
            .unwrap();

        time_provider.advance_secs(400);

        // Second checkpoint - use bookmark trigger to bypass change threshold checks
        let content2 = "Line 1\nLine 2 modified\nLine 3\nLine 4";
        let cp2 = manager
            .create_checkpoint(
                "test.md",
                content2,
                "{}",
                "bookmark",
                Some("Compare test"),
                None,
            )
            .await
            .unwrap();

        let (additions, deletions) = manager.compare_checkpoints(&cp1, &cp2).await.unwrap();

        assert!(additions.contains(&"Line 2 modified".to_string()));
        assert!(additions.contains(&"Line 4".to_string()));
        assert!(deletions.contains(&"Line 2".to_string()));
    }

    #[tokio::test]
    async fn test_path_to_key() {
        assert_eq!(
            CheckpointManager::<MockObjectStore, MockTimeProvider>::path_to_key("docs/notes.md"),
            "docs__notes_md"
        );
        assert_eq!(
            CheckpointManager::<MockObjectStore, MockTimeProvider>::path_to_key("file.txt"),
            "file_txt"
        );
        assert_eq!(
            CheckpointManager::<MockObjectStore, MockTimeProvider>::path_to_key("a/b/c.md"),
            "a__b__c_md"
        );
    }

    // ============================================================================
    // Additional Tests
    // ============================================================================

    #[test]
    fn test_checkpoint_config_default() {
        let config = CheckpointConfig::default();
        assert_eq!(config.min_interval_seconds, 300);
        assert_eq!(config.min_change_threshold, 50);
        assert_eq!(config.max_checkpoints_per_file, 50);
        assert_eq!(config.retention_days, 7);
    }

    #[test]
    fn test_checkpoint_serialization() {
        let checkpoint = Checkpoint {
            id: "cp-12345678".to_string(),
            content_hash: "abc123".to_string(),
            sidecar_hash: "def456".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            parent_id: Some("cp-previous".to_string()),
            checkpoint_type: "auto".to_string(),
            label: None,
            description: None,
            stats: CheckpointStats {
                word_count: 100,
                char_count: 500,
                change_size: 50,
            },
            trigger: "manual".to_string(),
        };

        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains("\"contentHash\":\"abc123\""));
        assert!(json.contains("\"sidecarHash\":\"def456\""));
        assert!(json.contains("\"parentId\":\"cp-previous\""));
        assert!(json.contains("\"wordCount\":100"));
    }

    #[test]
    fn test_checkpoint_deserialization() {
        let json = r#"{
            "id": "cp-test",
            "contentHash": "hash1",
            "sidecarHash": "hash2",
            "timestamp": "2024-01-01T00:00:00Z",
            "parentId": null,
            "type": "bookmark",
            "label": "My Label",
            "description": "My Description",
            "stats": { "wordCount": 10, "charCount": 50, "changeSize": 50 },
            "trigger": "bookmark"
        }"#;

        let checkpoint: Checkpoint = serde_json::from_str(json).unwrap();
        assert_eq!(checkpoint.id, "cp-test");
        assert_eq!(checkpoint.checkpoint_type, "bookmark");
        assert_eq!(checkpoint.label, Some("My Label".to_string()));
        assert_eq!(checkpoint.stats.word_count, 10);
    }

    #[test]
    fn test_checkpoint_history_serialization() {
        let history = CheckpointHistory {
            file_key: "test_md".to_string(),
            head_id: Some("cp-123".to_string()),
            checkpoints: vec![],
        };

        let json = serde_json::to_string(&history).unwrap();
        assert!(json.contains("\"fileKey\":\"test_md\""));
        assert!(json.contains("\"headId\":\"cp-123\""));
    }

    #[tokio::test]
    async fn test_init_creates_directory() {
        let (_temp, manager) = create_test_manager();
        manager.init().await.unwrap();

        assert!(manager.checkpoints_dir.exists());
    }

    #[tokio::test]
    async fn test_load_history_empty() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let history = manager.load_history("nonexistent.md").await.unwrap();
        assert!(history.checkpoints.is_empty());
        assert!(history.head_id.is_none());
    }

    #[tokio::test]
    async fn test_get_checkpoint_not_found() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let result = manager.get_checkpoint("test.md", "nonexistent").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Checkpoint not found"));
    }

    #[tokio::test]
    async fn test_get_checkpoint_success() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let created = manager
            .create_checkpoint("test.md", "Hello World", "{}", "manual", None, None)
            .await
            .unwrap();

        let retrieved = manager
            .get_checkpoint("test.md", &created.id)
            .await
            .unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.content_hash, created.content_hash);
    }

    #[tokio::test]
    async fn test_multiple_files_separate_histories() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        manager
            .create_checkpoint("file1.md", "Content for file 1", "{}", "manual", None, None)
            .await
            .unwrap();

        manager
            .create_checkpoint("file2.md", "Content for file 2", "{}", "manual", None, None)
            .await
            .unwrap();

        let checkpoints1 = manager.get_checkpoints("file1.md").await.unwrap();
        let checkpoints2 = manager.get_checkpoints("file2.md").await.unwrap();

        assert_eq!(checkpoints1.len(), 1);
        assert_eq!(checkpoints2.len(), 1);
        assert_ne!(checkpoints1[0].id, checkpoints2[0].id);
    }

    #[tokio::test]
    async fn test_parent_id_chain() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone());
        manager.init().await.unwrap();

        // First checkpoint
        let cp1 = manager
            .create_checkpoint(
                "test.md",
                "First content with sufficient length for threshold",
                "{}",
                "manual",
                None,
                None,
            )
            .await
            .unwrap();

        assert!(cp1.parent_id.is_none());

        time_provider.advance_secs(400);

        // Second checkpoint - use bookmark to bypass threshold
        let cp2 = manager
            .create_checkpoint(
                "test.md",
                "Second content",
                "{}",
                "bookmark",
                Some("V2"),
                None,
            )
            .await
            .unwrap();

        assert_eq!(cp2.parent_id, Some(cp1.id.clone()));

        time_provider.advance_secs(400);

        // Third checkpoint - use bookmark to bypass threshold
        let cp3 = manager
            .create_checkpoint(
                "test.md",
                "Third content",
                "{}",
                "bookmark",
                Some("V3"),
                None,
            )
            .await
            .unwrap();

        assert_eq!(cp3.parent_id, Some(cp2.id));
    }

    #[tokio::test]
    async fn test_stats_calculation() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let content = "Hello World\nThis is a test document";
        let checkpoint = manager
            .create_checkpoint("test.md", content, "{}", "manual", None, None)
            .await
            .unwrap();

        assert_eq!(checkpoint.stats.word_count, 7); // Hello, World, This, is, a, test, document
        assert_eq!(checkpoint.stats.char_count, content.len() as u32);
        assert_eq!(checkpoint.stats.change_size, content.len() as i32); // First checkpoint
    }

    #[tokio::test]
    async fn test_change_size_calculation() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone());
        manager.init().await.unwrap();

        // First checkpoint - 100 chars
        let content1 = "x".repeat(100);
        manager
            .create_checkpoint("test.md", &content1, "{}", "manual", None, None)
            .await
            .unwrap();

        time_provider.advance_secs(400);

        // Second checkpoint - 150 chars (use bookmark to bypass threshold)
        let content2 = "x".repeat(150);
        let cp2 = manager
            .create_checkpoint("test.md", &content2, "{}", "bookmark", Some("V2"), None)
            .await
            .unwrap();

        assert_eq!(cp2.stats.change_size, 50); // 150 - 100
    }

    #[tokio::test]
    async fn test_negative_change_size() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone());
        manager.init().await.unwrap();

        // First checkpoint - 200 chars
        let content1 = "x".repeat(200);
        manager
            .create_checkpoint("test.md", &content1, "{}", "manual", None, None)
            .await
            .unwrap();

        time_provider.advance_secs(400);

        // Second checkpoint - 100 chars (use bookmark)
        let content2 = "x".repeat(100);
        let cp2 = manager
            .create_checkpoint("test.md", &content2, "{}", "bookmark", Some("V2"), None)
            .await
            .unwrap();

        assert_eq!(cp2.stats.change_size, -100); // 100 - 200
    }

    #[tokio::test]
    async fn test_bookmark_bypasses_interval_check() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        // First checkpoint
        let cp1 = manager
            .create_checkpoint("test.md", "Content 1", "{}", "manual", None, None)
            .await
            .unwrap();

        // Immediately create bookmark (no time advance)
        let cp2 = manager
            .create_checkpoint(
                "test.md",
                "Content 2",
                "{}",
                "bookmark",
                Some("Important"),
                None,
            )
            .await
            .unwrap();

        // Should be different checkpoints even though interval hasn't passed
        assert_ne!(cp1.id, cp2.id);
        assert_eq!(cp2.checkpoint_type, "bookmark");
    }

    #[tokio::test]
    async fn test_max_checkpoints_limit() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone())
                .with_config(CheckpointConfig {
                    min_interval_seconds: 1,
                    min_change_threshold: 1,
                    max_checkpoints_per_file: 5,
                    retention_days: 365, // Long retention to avoid age-based removal
                });
        manager.init().await.unwrap();

        // Create 10 checkpoints
        for i in 0..10 {
            time_provider.advance_secs(2);
            let content = format!("Content version {} with unique content", i);
            manager
                .create_checkpoint("test.md", &content, "{}", "manual", None, None)
                .await
                .unwrap();
        }

        let checkpoints = manager.get_checkpoints("test.md").await.unwrap();
        assert!(checkpoints.len() <= 5);
    }

    #[tokio::test]
    async fn test_bookmarks_count_towards_limit_but_preserved() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let mut manager =
            CheckpointManager::with_deps(temp.path(), object_store, time_provider.clone())
                .with_config(CheckpointConfig {
                    min_interval_seconds: 1,
                    min_change_threshold: 1,
                    max_checkpoints_per_file: 5,
                    retention_days: 365,
                });
        manager.init().await.unwrap();

        // Create 3 bookmarks
        for i in 0..3 {
            time_provider.advance_secs(2);
            manager
                .create_checkpoint(
                    "test.md",
                    &format!("Bookmark content {}", i),
                    "{}",
                    "bookmark",
                    Some(&format!("BM{}", i)),
                    None,
                )
                .await
                .unwrap();
        }

        // Create 5 auto checkpoints
        for i in 0..5 {
            time_provider.advance_secs(2);
            manager
                .create_checkpoint(
                    "test.md",
                    &format!("Auto content {} with extra", i),
                    "{}",
                    "manual",
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        let checkpoints = manager.get_checkpoints("test.md").await.unwrap();

        // All 3 bookmarks should be preserved
        let bookmark_count = checkpoints
            .iter()
            .filter(|cp| cp.checkpoint_type == "bookmark")
            .count();
        assert_eq!(bookmark_count, 3);
    }

    #[tokio::test]
    async fn test_with_config() {
        let (_temp, manager) = create_test_manager();

        let custom_config = CheckpointConfig {
            min_interval_seconds: 60,
            min_change_threshold: 100,
            max_checkpoints_per_file: 10,
            retention_days: 30,
        };

        let manager = manager.with_config(custom_config);

        assert_eq!(manager.config().min_interval_seconds, 60);
        assert_eq!(manager.config().min_change_threshold, 100);
        assert_eq!(manager.config().max_checkpoints_per_file, 10);
        assert_eq!(manager.config().retention_days, 30);
    }

    #[tokio::test]
    async fn test_history_persisted_to_disk() {
        let temp = tempdir().unwrap();
        let object_store = Arc::new(MockObjectStore::new());
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        {
            let mut manager = CheckpointManager::with_deps(
                temp.path(),
                object_store.clone(),
                time_provider.clone(),
            );
            manager.init().await.unwrap();

            manager
                .create_checkpoint("test.md", "Hello World", "{}", "manual", None, None)
                .await
                .unwrap();
        }

        // Create a new manager (simulating app restart)
        let mut manager2 = CheckpointManager::with_deps(temp.path(), object_store, time_provider);

        let checkpoints = manager2.get_checkpoints("test.md").await.unwrap();
        assert_eq!(checkpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_compare_identical_checkpoints() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let content = "Same content";
        let cp1 = manager
            .create_checkpoint("test.md", content, "{}", "bookmark", Some("V1"), None)
            .await
            .unwrap();

        let (additions, deletions) = manager.compare_checkpoints(&cp1, &cp1).await.unwrap();

        assert!(additions.is_empty());
        assert!(deletions.is_empty());
    }

    #[test]
    fn test_path_to_key_windows_separator() {
        assert_eq!(
            CheckpointManager::<MockObjectStore, MockTimeProvider>::path_to_key("docs\\notes.md"),
            "docs__notes_md"
        );
    }

    #[test]
    fn test_path_to_key_mixed_separators() {
        assert_eq!(
            CheckpointManager::<MockObjectStore, MockTimeProvider>::path_to_key(
                "docs/sub\\file.md"
            ),
            "docs__sub__file_md"
        );
    }

    #[test]
    fn test_path_to_key_multiple_dots() {
        assert_eq!(
            CheckpointManager::<MockObjectStore, MockTimeProvider>::path_to_key("file.test.md"),
            "file_test_md"
        );
    }

    #[tokio::test]
    async fn test_empty_markdown_checkpoint() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        let checkpoint = manager
            .create_checkpoint("test.md", "", "{}", "manual", None, None)
            .await
            .unwrap();

        assert_eq!(checkpoint.stats.word_count, 0);
        assert_eq!(checkpoint.stats.char_count, 0);
    }

    #[tokio::test]
    async fn test_unicode_content_stats() {
        let (_temp, mut manager) = create_test_manager();
        manager.init().await.unwrap();

        // 2 Japanese words, 6 bytes per character but should count chars correctly
        let content = "こんにちは 世界"; // "Hello World" in Japanese
        let checkpoint = manager
            .create_checkpoint("test.md", content, "{}", "manual", None, None)
            .await
            .unwrap();

        assert_eq!(checkpoint.stats.word_count, 2);
        // char_count is bytes, not unicode chars
        assert_eq!(checkpoint.stats.char_count, content.len() as u32);
    }
}
