// Checkpoint manager - Version history with retention policies

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::error::{MidlightError, Result};
use super::object_store::ObjectStore;

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
            min_interval_seconds: 300,  // 5 minutes
            min_change_threshold: 50,   // 50 characters
            max_checkpoints_per_file: 50,
            retention_days: 7,
        }
    }
}

/// Manages version history for documents
pub struct CheckpointManager {
    checkpoints_dir: PathBuf,
    object_store: ObjectStore,
    config: CheckpointConfig,
    histories: HashMap<String, CheckpointHistory>,
}

impl CheckpointManager {
    pub fn new(workspace_root: &Path, object_store: ObjectStore) -> Self {
        Self {
            checkpoints_dir: workspace_root.join(".midlight").join("checkpoints"),
            object_store,
            config: CheckpointConfig::default(),
            histories: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_config(mut self, config: CheckpointConfig) -> Self {
        self.config = config;
        self
    }

    /// Initialize the checkpoints directory
    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.checkpoints_dir)?;
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
                let content = fs::read_to_string(&history_path)?;
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
        fs::write(history_path, content)?;
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
        if trigger != "bookmark" && !Self::should_create_checkpoint(&self.config, &history, markdown) {
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
        let id = format!("cp-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
        let timestamp = Utc::now().to_rfc3339();

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
        Self::apply_retention_policy(&self.config, &mut history);

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
    fn should_create_checkpoint(config: &CheckpointConfig, history: &CheckpointHistory, markdown: &str) -> bool {
        if history.checkpoints.is_empty() {
            return true;
        }

        // Check time since last checkpoint
        if let Some(head_id) = &history.head_id {
            if let Some(last) = history.checkpoints.iter().find(|c| &c.id == head_id) {
                if let Ok(last_time) = DateTime::parse_from_rfc3339(&last.timestamp) {
                    let elapsed = Utc::now().signed_duration_since(last_time.with_timezone(&Utc));
                    if elapsed < Duration::seconds(config.min_interval_seconds as i64) {
                        return false;
                    }
                }

                // Check change size
                let change_size = (markdown.len() as i32 - last.stats.char_count as i32).unsigned_abs();
                if change_size < config.min_change_threshold {
                    return false;
                }
            }
        }

        true
    }

    /// Apply retention policy to checkpoint history
    fn apply_retention_policy(config: &CheckpointConfig, history: &mut CheckpointHistory) {
        let now = Utc::now();
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
            let slots_remaining = config.max_checkpoints_per_file.saturating_sub(to_keep.len());
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
    pub async fn get_checkpoint(&mut self, file_path: &str, checkpoint_id: &str) -> Result<Checkpoint> {
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
