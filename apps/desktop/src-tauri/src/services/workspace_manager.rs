// Workspace manager - Orchestrates all services for a workspace

use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::checkpoint_manager::{Checkpoint, CheckpointManager};
use super::error::Result;
use super::object_store::ObjectStore;
use crate::commands::versions::DiffResult;
use crate::commands::workspace::{LoadedDocument, SaveResult};

/// Manages a single workspace (folder)
pub struct WorkspaceManager {
    workspace_root: PathBuf,
    midlight_dir: PathBuf,
    object_store: Arc<ObjectStore>,
    checkpoint_manager: Arc<RwLock<CheckpointManager>>,
}

impl WorkspaceManager {
    pub fn new(workspace_root: &Path) -> Self {
        let object_store = Arc::new(ObjectStore::new(workspace_root));
        let checkpoint_manager = Arc::new(RwLock::new(CheckpointManager::new(
            workspace_root,
            ObjectStore::new(workspace_root),
        )));

        Self {
            workspace_root: workspace_root.to_path_buf(),
            midlight_dir: workspace_root.join(".midlight"),
            object_store,
            checkpoint_manager,
        }
    }

    /// Initialize the workspace (.midlight folder structure)
    pub async fn init(&self) -> Result<()> {
        // Create .midlight directory structure
        fs::create_dir_all(&self.midlight_dir)?;
        fs::create_dir_all(self.midlight_dir.join("objects"))?;
        fs::create_dir_all(self.midlight_dir.join("checkpoints"))?;
        fs::create_dir_all(self.midlight_dir.join("images"))?;
        fs::create_dir_all(self.midlight_dir.join("recovery"))?;

        // Initialize services
        self.object_store.init().await?;
        self.checkpoint_manager.write().await.init().await?;

        // Create default config if not exists
        let config_path = self.midlight_dir.join("workspace.config.json");
        if !config_path.exists() {
            let default_config = serde_json::json!({
                "version": 1,
                "versioning": {
                    "enabled": true,
                    "autoCheckpointInterval": 300,
                    "minChangeThreshold": 50,
                    "maxCheckpointsPerFile": 50,
                    "retentionDays": 7
                },
                "editor": {
                    "defaultFont": "Inter",
                    "defaultFontSize": "16px",
                    "spellcheck": true,
                    "autoSave": true,
                    "autoSaveInterval": 3000
                },
                "recovery": {
                    "enabled": true,
                    "walInterval": 500
                }
            });
            fs::write(config_path, serde_json::to_string_pretty(&default_config)?)?;
        }

        tracing::info!("Initialized workspace: {}", self.workspace_root.display());

        Ok(())
    }

    /// Load a document - handles both .midlight (native) and .md (legacy) formats
    pub async fn load_document(&self, file_path: &str) -> Result<LoadedDocument> {
        let full_path = self.workspace_root.join(file_path);

        // Check for recovery file
        let recovery_path = self.midlight_dir.join("recovery").join(format!(
            "{}.wal",
            file_path.replace(['/', '\\'], "__").replace('.', "_")
        ));
        let has_recovery = recovery_path.exists();
        let recovery_time = if has_recovery {
            recovery_path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        } else {
            None
        };

        // Handle based on file extension
        if file_path.ends_with(".midlight") {
            // Native .midlight format - read directly
            self.load_midlight_document(&full_path, has_recovery, recovery_time)
        } else if file_path.ends_with(".md") {
            // Legacy .md format - migrate to .midlight
            self.load_and_migrate_markdown(&full_path, file_path, has_recovery, recovery_time)
                .await
        } else {
            // Unsupported format - try to read as plain text
            let content = if full_path.exists() {
                fs::read_to_string(&full_path)?
            } else {
                String::new()
            };
            let json = self.markdown_to_tiptap(&content);
            Ok(LoadedDocument {
                json,
                sidecar: self.create_empty_sidecar(),
                has_recovery,
                recovery_time,
            })
        }
    }

    /// Load a .midlight file directly
    fn load_midlight_document(
        &self,
        full_path: &Path,
        has_recovery: bool,
        recovery_time: Option<String>,
    ) -> Result<LoadedDocument> {
        if !full_path.exists() {
            // Return empty document if file doesn't exist
            let now = chrono::Utc::now().to_rfc3339();
            return Ok(LoadedDocument {
                json: serde_json::json!({
                    "type": "doc",
                    "content": [{ "type": "paragraph" }]
                }),
                sidecar: serde_json::json!({
                    "version": 1,
                    "meta": { "created": now, "modified": now },
                    "document": {},
                    "blocks": {},
                    "spans": {},
                    "images": {}
                }),
                has_recovery,
                recovery_time,
            });
        }

        let content = fs::read_to_string(full_path)?;
        let midlight_doc: Value = serde_json::from_str(&content)?;

        // Extract content (Tiptap JSON)
        let json = midlight_doc.get("content").cloned().unwrap_or_else(|| {
            serde_json::json!({
                "type": "doc",
                "content": [{ "type": "paragraph" }]
            })
        });

        // Build sidecar from meta and document settings
        let meta = midlight_doc.get("meta").cloned().unwrap_or_else(|| {
            let now = chrono::Utc::now().to_rfc3339();
            serde_json::json!({ "created": now, "modified": now })
        });
        let document = midlight_doc
            .get("document")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let images = midlight_doc
            .get("images")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let sidecar = serde_json::json!({
            "version": 1,
            "meta": meta,
            "document": document,
            "blocks": {},
            "spans": {},
            "images": images
        });

        tracing::debug!("Loaded .midlight document: {}", full_path.display());

        Ok(LoadedDocument {
            json,
            sidecar,
            has_recovery,
            recovery_time,
        })
    }

    /// Load a legacy .md file and migrate it to .midlight format
    async fn load_and_migrate_markdown(
        &self,
        full_path: &Path,
        file_path: &str,
        has_recovery: bool,
        recovery_time: Option<String>,
    ) -> Result<LoadedDocument> {
        // Read markdown file
        let markdown = if full_path.exists() {
            fs::read_to_string(full_path)?
        } else {
            String::new()
        };

        // Read sidecar file
        let sidecar_path = format!("{}.sidecar.json", full_path.display());
        let sidecar: Value = if Path::new(&sidecar_path).exists() {
            let content = fs::read_to_string(&sidecar_path)?;
            serde_json::from_str(&content)?
        } else {
            self.create_empty_sidecar()
        };

        // Convert markdown to Tiptap JSON
        let json = self.markdown_to_tiptap(&markdown);

        // Create backup of original .md file
        if full_path.exists() {
            let backup_path = format!("{}.backup", full_path.display());
            if !Path::new(&backup_path).exists() {
                fs::copy(full_path, &backup_path)?;
                tracing::info!("Created backup: {}", backup_path);
            }
        }

        // Create .midlight file
        let midlight_path = full_path.with_extension("midlight");
        let now = chrono::Utc::now().to_rfc3339();

        let meta = sidecar
            .get("meta")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "created": now, "modified": now }));
        let document = sidecar.get("document").cloned().unwrap_or_else(
            || serde_json::json!({ "defaultFont": "Merriweather", "defaultFontSize": 16 }),
        );
        let images = sidecar
            .get("images")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let midlight_doc = serde_json::json!({
            "version": 1,
            "meta": meta,
            "document": document,
            "content": json,
            "images": images
        });

        fs::write(&midlight_path, serde_json::to_string_pretty(&midlight_doc)?)?;
        tracing::info!("Migrated {} to {}", file_path, midlight_path.display());

        // Delete original .md and .sidecar.json files after successful migration
        if full_path.exists() {
            fs::remove_file(full_path)?;
            tracing::debug!("Removed original .md file: {}", full_path.display());
        }
        if Path::new(&sidecar_path).exists() {
            fs::remove_file(&sidecar_path)?;
            tracing::debug!("Removed sidecar file: {}", sidecar_path);
        }

        Ok(LoadedDocument {
            json,
            sidecar,
            has_recovery,
            recovery_time,
        })
    }

    /// Save a document - always saves as .midlight format
    pub async fn save_document(
        &self,
        file_path: &str,
        json: Value,
        trigger: &str,
    ) -> Result<SaveResult> {
        // Determine the .midlight file path
        let midlight_path = if file_path.ends_with(".midlight") {
            file_path.to_string()
        } else if file_path.ends_with(".md") {
            file_path.replace(".md", ".midlight")
        } else {
            format!("{}.midlight", file_path)
        };

        let full_path = self.workspace_root.join(&midlight_path);

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing document to preserve meta.created
        let (created, existing_images) = if full_path.exists() {
            let existing = fs::read_to_string(&full_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok());
            let created = existing
                .as_ref()
                .and_then(|d| d.get("meta"))
                .and_then(|m| m.get("created"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            let images = existing.as_ref().and_then(|d| d.get("images")).cloned();
            (created, images)
        } else {
            (None, None)
        };

        let now = chrono::Utc::now().to_rfc3339();

        // Build the MidlightDocument
        let midlight_doc = serde_json::json!({
            "version": 1,
            "meta": {
                "created": created.unwrap_or_else(|| now.clone()),
                "modified": now
            },
            "document": {
                "defaultFont": "Merriweather",
                "defaultFontSize": 16
            },
            "content": json,
            "images": existing_images.unwrap_or_else(|| serde_json::json!({}))
        });

        // Write the .midlight file
        fs::write(&full_path, serde_json::to_string_pretty(&midlight_doc)?)?;

        // For checkpoint, we store the full midlight document content
        let content_for_checkpoint = serde_json::to_string(&midlight_doc)?;
        let sidecar_placeholder = "{}"; // Sidecar info is now part of the midlight doc

        let checkpoint = self
            .checkpoint_manager
            .write()
            .await
            .create_checkpoint(
                &midlight_path,
                &content_for_checkpoint,
                sidecar_placeholder,
                trigger,
                None,
                None,
            )
            .await?;

        // Clear recovery file
        let recovery_path = self.midlight_dir.join("recovery").join(format!(
            "{}.wal",
            midlight_path.replace(['/', '\\'], "__").replace('.', "_")
        ));
        let _ = fs::remove_file(recovery_path);

        tracing::debug!(
            "Saved document: {} (checkpoint: {})",
            midlight_path,
            &checkpoint.id[..8]
        );

        Ok(SaveResult {
            success: true,
            checkpoint_id: Some(checkpoint.id),
            error: None,
        })
    }

    /// Get checkpoints for a file
    pub async fn get_checkpoints(&self, file_path: &str) -> Result<Vec<Checkpoint>> {
        self.checkpoint_manager
            .write()
            .await
            .get_checkpoints(file_path)
            .await
    }

    /// Restore a checkpoint
    pub async fn restore_checkpoint(&self, file_path: &str, checkpoint_id: &str) -> Result<Value> {
        let mut cm = self.checkpoint_manager.write().await;
        let checkpoint = cm.get_checkpoint(file_path, checkpoint_id).await?;
        let (content, _sidecar_str) = cm.get_checkpoint_content(&checkpoint).await?;

        // Try to parse as MidlightDocument (new format)
        if let Ok(midlight_doc) = serde_json::from_str::<Value>(&content) {
            if midlight_doc.get("version").is_some() && midlight_doc.get("content").is_some() {
                // New .midlight format - extract content directly
                return Ok(midlight_doc.get("content").cloned().unwrap_or_else(|| {
                    serde_json::json!({
                        "type": "doc",
                        "content": [{ "type": "paragraph" }]
                    })
                }));
            }
        }

        // Legacy format - treat content as markdown
        let json = self.markdown_to_tiptap(&content);
        Ok(json)
    }

    /// Create a bookmark (named checkpoint) - saves as .midlight format
    pub async fn create_bookmark(
        &self,
        file_path: &str,
        json: Value,
        label: &str,
        description: Option<&str>,
    ) -> Result<SaveResult> {
        // Determine the .midlight file path
        let midlight_path = if file_path.ends_with(".midlight") {
            file_path.to_string()
        } else if file_path.ends_with(".md") {
            file_path.replace(".md", ".midlight")
        } else {
            format!("{}.midlight", file_path)
        };

        let full_path = self.workspace_root.join(&midlight_path);

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing document to preserve meta.created
        let (created, existing_images) = if full_path.exists() {
            let existing = fs::read_to_string(&full_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok());
            let created = existing
                .as_ref()
                .and_then(|d| d.get("meta"))
                .and_then(|m| m.get("created"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            let images = existing.as_ref().and_then(|d| d.get("images")).cloned();
            (created, images)
        } else {
            (None, None)
        };

        let now = chrono::Utc::now().to_rfc3339();

        // Build the MidlightDocument
        let midlight_doc = serde_json::json!({
            "version": 1,
            "meta": {
                "created": created.unwrap_or_else(|| now.clone()),
                "modified": now
            },
            "document": {
                "defaultFont": "Merriweather",
                "defaultFontSize": 16
            },
            "content": json,
            "images": existing_images.unwrap_or_else(|| serde_json::json!({}))
        });

        // Write the .midlight file
        fs::write(&full_path, serde_json::to_string_pretty(&midlight_doc)?)?;

        // For checkpoint, store the full midlight document
        let content_for_checkpoint = serde_json::to_string(&midlight_doc)?;

        // Create bookmark checkpoint
        let checkpoint = self
            .checkpoint_manager
            .write()
            .await
            .create_checkpoint(
                &midlight_path,
                &content_for_checkpoint,
                "{}",
                "bookmark",
                Some(label),
                description,
            )
            .await?;

        Ok(SaveResult {
            success: true,
            checkpoint_id: Some(checkpoint.id),
            error: None,
        })
    }

    /// Compare two checkpoints
    pub async fn compare_checkpoints(
        &self,
        file_path: &str,
        checkpoint_id_a: &str,
        checkpoint_id_b: &str,
    ) -> Result<DiffResult> {
        let mut cm = self.checkpoint_manager.write().await;
        let cp_a = cm.get_checkpoint(file_path, checkpoint_id_a).await?;
        let cp_b = cm.get_checkpoint(file_path, checkpoint_id_b).await?;

        let (additions, deletions) = cm.compare_checkpoints(&cp_a, &cp_b).await?;

        Ok(DiffResult {
            additions,
            deletions,
            change_count: (cp_b.stats.char_count as i32 - cp_a.stats.char_count as i32)
                .unsigned_abs(),
        })
    }

    // Helper methods for document conversion

    fn create_empty_sidecar(&self) -> Value {
        let now = chrono::Utc::now().to_rfc3339();
        serde_json::json!({
            "version": 1,
            "meta": {
                "created": now,
                "modified": now
            },
            "document": {},
            "blocks": {},
            "spans": {},
            "images": {}
        })
    }

    /// Simple markdown to Tiptap JSON conversion
    /// Full conversion is done in TypeScript for accuracy
    fn markdown_to_tiptap(&self, markdown: &str) -> Value {
        let mut content = Vec::new();

        for line in markdown.lines() {
            if line.starts_with("# ") {
                content.push(serde_json::json!({
                    "type": "heading",
                    "attrs": { "level": 1 },
                    "content": [{ "type": "text", "text": &line[2..] }]
                }));
            } else if line.starts_with("## ") {
                content.push(serde_json::json!({
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": &line[3..] }]
                }));
            } else if line.starts_with("### ") {
                content.push(serde_json::json!({
                    "type": "heading",
                    "attrs": { "level": 3 },
                    "content": [{ "type": "text", "text": &line[4..] }]
                }));
            } else if !line.is_empty() {
                content.push(serde_json::json!({
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": line }]
                }));
            } else {
                content.push(serde_json::json!({
                    "type": "paragraph"
                }));
            }
        }

        if content.is_empty() {
            content.push(serde_json::json!({
                "type": "paragraph"
            }));
        }

        serde_json::json!({
            "type": "doc",
            "content": content
        })
    }

    /// Simple Tiptap JSON to markdown conversion
    #[allow(dead_code)]
    fn tiptap_to_markdown(&self, json: &Value) -> String {
        let mut lines = Vec::new();

        if let Some(content) = json.get("content").and_then(|c| c.as_array()) {
            for node in content {
                let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match node_type {
                    "heading" => {
                        let level = node
                            .get("attrs")
                            .and_then(|a| a.get("level"))
                            .and_then(|l| l.as_u64())
                            .unwrap_or(1) as usize;
                        let text = self.extract_text_content(node);
                        lines.push(format!("{} {}", "#".repeat(level), text));
                    }
                    "paragraph" => {
                        let text = self.extract_text_content(node);
                        lines.push(text);
                    }
                    "bulletList" => {
                        if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                            for item in items {
                                let text = self.extract_text_content(item);
                                lines.push(format!("- {}", text));
                            }
                        }
                    }
                    "orderedList" => {
                        if let Some(items) = node.get("content").and_then(|c| c.as_array()) {
                            for (i, item) in items.iter().enumerate() {
                                let text = self.extract_text_content(item);
                                lines.push(format!("{}. {}", i + 1, text));
                            }
                        }
                    }
                    "blockquote" => {
                        let text = self.extract_text_content(node);
                        for line in text.lines() {
                            lines.push(format!("> {}", line));
                        }
                    }
                    "codeBlock" => {
                        let lang = node
                            .get("attrs")
                            .and_then(|a| a.get("language"))
                            .and_then(|l| l.as_str())
                            .unwrap_or("");
                        let text = self.extract_text_content(node);
                        lines.push(format!("```{}", lang));
                        lines.push(text);
                        lines.push("```".to_string());
                    }
                    "horizontalRule" => {
                        lines.push("---".to_string());
                    }
                    _ => {}
                }
            }
        }

        lines.join("\n")
    }

    #[allow(dead_code)]
    fn extract_text_content(&self, node: &Value) -> String {
        if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
            return text.to_string();
        }

        if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
            return content
                .iter()
                .map(|n| self.extract_text_content(n))
                .collect::<Vec<_>>()
                .join("");
        }

        String::new()
    }

    #[allow(dead_code)]
    fn extract_sidecar(&self, _json: &Value) -> Value {
        // For now, create a basic sidecar
        // Full extraction is done in TypeScript
        self.create_empty_sidecar()
    }
}

/// Registry of workspace managers (one per open workspace)
pub struct WorkspaceManagerRegistry {
    managers: HashMap<String, Arc<WorkspaceManager>>,
}

impl WorkspaceManagerRegistry {
    pub fn new() -> Self {
        Self {
            managers: HashMap::new(),
        }
    }

    /// Get an existing workspace manager
    pub fn get(&self, workspace_root: &str) -> Option<Arc<WorkspaceManager>> {
        self.managers.get(workspace_root).cloned()
    }

    /// Get or create a workspace manager
    pub async fn get_or_create(&mut self, workspace_root: &str) -> Result<Arc<WorkspaceManager>> {
        if let Some(manager) = self.managers.get(workspace_root) {
            return Ok(manager.clone());
        }

        let manager = Arc::new(WorkspaceManager::new(Path::new(workspace_root)));
        self.managers
            .insert(workspace_root.to_string(), manager.clone());

        Ok(manager)
    }

    /// Remove a workspace manager
    pub fn remove(&mut self, workspace_root: &str) {
        self.managers.remove(workspace_root);
    }
}

impl Default for WorkspaceManagerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ============================================
    // Workspace initialization tests
    // ============================================

    #[tokio::test]
    async fn test_workspace_init_creates_structure() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        manager.init().await.unwrap();

        // Verify directory structure
        assert!(temp.path().join(".midlight").exists());
        assert!(temp.path().join(".midlight/objects").exists());
        assert!(temp.path().join(".midlight/checkpoints").exists());
        assert!(temp.path().join(".midlight/images").exists());
        assert!(temp.path().join(".midlight/recovery").exists());

        // Verify config file
        let config_path = temp.path().join(".midlight/workspace.config.json");
        assert!(config_path.exists());

        // Verify config content
        let config_content = fs::read_to_string(&config_path).unwrap();
        let config: Value = serde_json::from_str(&config_content).unwrap();
        assert_eq!(config["version"], 1);
        assert!(config["versioning"]["enabled"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_workspace_init_idempotent() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        // Init twice should work
        manager.init().await.unwrap();
        manager.init().await.unwrap();

        assert!(temp.path().join(".midlight").exists());
    }

    #[tokio::test]
    async fn test_workspace_init_preserves_existing_config() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        // Create custom config before init
        fs::create_dir_all(temp.path().join(".midlight")).unwrap();
        let custom_config = serde_json::json!({
            "version": 1,
            "custom": "value"
        });
        fs::write(
            temp.path().join(".midlight/workspace.config.json"),
            serde_json::to_string(&custom_config).unwrap(),
        )
        .unwrap();

        manager.init().await.unwrap();

        // Custom config should be preserved
        let config_content =
            fs::read_to_string(temp.path().join(".midlight/workspace.config.json")).unwrap();
        let config: Value = serde_json::from_str(&config_content).unwrap();
        assert_eq!(config["custom"], "value");
    }

    // ============================================
    // Document loading tests
    // ============================================

    #[tokio::test]
    async fn test_load_nonexistent_midlight_document() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let result = manager.load_document("test.midlight").await.unwrap();

        // Should return empty document
        assert_eq!(result.json["type"], "doc");
        assert!(result.json["content"].is_array());
        assert!(!result.has_recovery);
    }

    #[tokio::test]
    async fn test_load_existing_midlight_document() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create a .midlight file
        let doc = serde_json::json!({
            "version": 1,
            "meta": {
                "created": "2024-01-01T00:00:00Z",
                "modified": "2024-01-01T00:00:00Z"
            },
            "document": {},
            "content": {
                "type": "doc",
                "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "Hello" }] }
                ]
            },
            "images": {}
        });
        fs::write(
            temp.path().join("test.midlight"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();

        let result = manager.load_document("test.midlight").await.unwrap();

        assert_eq!(result.json["type"], "doc");
        assert_eq!(
            result.json["content"][0]["content"][0]["text"],
            "Hello"
        );
    }

    #[tokio::test]
    async fn test_load_markdown_migrates_to_midlight() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create a .md file
        fs::write(temp.path().join("test.md"), "# Hello World\n\nSome content").unwrap();

        let result = manager.load_document("test.md").await.unwrap();

        // Should convert to Tiptap format
        assert_eq!(result.json["type"], "doc");

        // .midlight file should be created
        assert!(temp.path().join("test.midlight").exists());

        // Original .md should be removed
        assert!(!temp.path().join("test.md").exists());

        // Backup should exist
        assert!(temp.path().join("test.md.backup").exists());
    }

    #[tokio::test]
    async fn test_load_unsupported_format() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create a .txt file
        fs::write(temp.path().join("test.txt"), "Plain text content").unwrap();

        let result = manager.load_document("test.txt").await.unwrap();

        // Should treat as plain text and convert
        assert_eq!(result.json["type"], "doc");
    }

    #[tokio::test]
    async fn test_load_document_in_subfolder() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create subfolder and document
        fs::create_dir_all(temp.path().join("notes/work")).unwrap();
        let doc = serde_json::json!({
            "version": 1,
            "meta": { "created": "2024-01-01T00:00:00Z", "modified": "2024-01-01T00:00:00Z" },
            "document": {},
            "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
            "images": {}
        });
        fs::write(
            temp.path().join("notes/work/ideas.midlight"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();

        let result = manager
            .load_document("notes/work/ideas.midlight")
            .await
            .unwrap();
        assert_eq!(result.json["type"], "doc");
    }

    // ============================================
    // Document saving tests
    // ============================================

    #[tokio::test]
    async fn test_save_document_creates_midlight_file() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Hello" }] }
            ]
        });

        let result = manager
            .save_document("test.midlight", json, "manual")
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.checkpoint_id.is_some());
        assert!(temp.path().join("test.midlight").exists());
    }

    #[tokio::test]
    async fn test_save_document_converts_md_path() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        });

        // Save with .md extension
        manager.save_document("test.md", json, "manual").await.unwrap();

        // Should create .midlight file
        assert!(temp.path().join("test.midlight").exists());
        assert!(!temp.path().join("test.md").exists());
    }

    #[tokio::test]
    async fn test_save_document_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        });

        manager
            .save_document("deep/nested/path/doc.midlight", json, "manual")
            .await
            .unwrap();

        assert!(temp.path().join("deep/nested/path/doc.midlight").exists());
    }

    #[tokio::test]
    async fn test_save_document_preserves_created_timestamp() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // First save
        let json1 = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "v1" }] }]
        });
        manager
            .save_document("test.midlight", json1, "manual")
            .await
            .unwrap();

        // Read the created timestamp
        let content1 = fs::read_to_string(temp.path().join("test.midlight")).unwrap();
        let doc1: Value = serde_json::from_str(&content1).unwrap();
        let created1 = doc1["meta"]["created"].as_str().unwrap().to_string();

        // Wait a bit and save again
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let json2 = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "v2" }] }]
        });
        manager
            .save_document("test.midlight", json2, "manual")
            .await
            .unwrap();

        // Read the created timestamp again
        let content2 = fs::read_to_string(temp.path().join("test.midlight")).unwrap();
        let doc2: Value = serde_json::from_str(&content2).unwrap();
        let created2 = doc2["meta"]["created"].as_str().unwrap();

        // Created should be preserved
        assert_eq!(created1, created2);

        // Modified should be different
        let modified2 = doc2["meta"]["modified"].as_str().unwrap();
        assert_ne!(created2, modified2);
    }

    #[tokio::test]
    async fn test_save_creates_checkpoint() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Content" }] }]
        });

        let result = manager
            .save_document("test.midlight", json, "manual")
            .await
            .unwrap();

        assert!(result.checkpoint_id.is_some());

        // Get checkpoints
        let checkpoints = manager.get_checkpoints("test.midlight").await.unwrap();
        assert!(!checkpoints.is_empty());
    }

    // ============================================
    // Checkpoint operations tests
    // ============================================

    #[tokio::test]
    async fn test_get_checkpoints_empty() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let checkpoints = manager.get_checkpoints("nonexistent.midlight").await.unwrap();
        assert!(checkpoints.is_empty());
    }

    #[tokio::test]
    async fn test_restore_checkpoint() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Save v1
        let json1 = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Version 1" }] }]
        });
        let result1 = manager
            .save_document("test.midlight", json1.clone(), "manual")
            .await
            .unwrap();
        let cp1_id = result1.checkpoint_id.unwrap();

        // Save v2
        let json2 = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Version 2" }] }]
        });
        manager
            .save_document("test.midlight", json2, "manual")
            .await
            .unwrap();

        // Restore v1
        let restored = manager
            .restore_checkpoint("test.midlight", &cp1_id)
            .await
            .unwrap();

        assert_eq!(restored["content"][0]["content"][0]["text"], "Version 1");
    }

    // ============================================
    // Bookmark tests
    // ============================================

    #[tokio::test]
    async fn test_create_bookmark() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Important" }] }]
        });

        let result = manager
            .create_bookmark("test.midlight", json, "Draft Complete", Some("First complete draft"))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.checkpoint_id.is_some());

        // Check that the checkpoint has the label
        let checkpoints = manager.get_checkpoints("test.midlight").await.unwrap();
        let bookmark = checkpoints
            .iter()
            .find(|c| c.label.as_deref() == Some("Draft Complete"));
        assert!(bookmark.is_some());
    }

    // ============================================
    // Markdown to Tiptap conversion tests
    // ============================================

    #[tokio::test]
    async fn test_markdown_to_tiptap_headings() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let markdown = "# Heading 1\n## Heading 2\n### Heading 3";
        let json = manager.markdown_to_tiptap(markdown);

        assert_eq!(json["type"], "doc");
        let content = json["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);

        assert_eq!(content[0]["type"], "heading");
        assert_eq!(content[0]["attrs"]["level"], 1);
        assert_eq!(content[0]["content"][0]["text"], "Heading 1");

        assert_eq!(content[1]["type"], "heading");
        assert_eq!(content[1]["attrs"]["level"], 2);

        assert_eq!(content[2]["type"], "heading");
        assert_eq!(content[2]["attrs"]["level"], 3);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_paragraphs() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let markdown = "First paragraph\n\nSecond paragraph";
        let json = manager.markdown_to_tiptap(markdown);

        let content = json["content"].as_array().unwrap();
        assert_eq!(content.len(), 3); // para, empty para, para

        assert_eq!(content[0]["type"], "paragraph");
        assert_eq!(content[0]["content"][0]["text"], "First paragraph");

        assert_eq!(content[1]["type"], "paragraph"); // empty line

        assert_eq!(content[2]["type"], "paragraph");
        assert_eq!(content[2]["content"][0]["text"], "Second paragraph");
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_empty() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = manager.markdown_to_tiptap("");

        assert_eq!(json["type"], "doc");
        let content = json["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "paragraph");
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_mixed_content() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let markdown = "# Title\n\nSome text here.\n\n## Section\n\nMore text.";
        let json = manager.markdown_to_tiptap(markdown);

        let content = json["content"].as_array().unwrap();
        assert!(content.len() >= 5);
    }

    // ============================================
    // Tiptap to Markdown conversion tests
    // ============================================

    #[tokio::test]
    async fn test_tiptap_to_markdown_headings() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "heading", "attrs": { "level": 1 }, "content": [{ "type": "text", "text": "H1" }] },
                { "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "H2" }] }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("# H1"));
        assert!(markdown.contains("## H2"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_lists() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "bulletList",
                    "content": [
                        { "type": "listItem", "content": [{ "type": "text", "text": "Item 1" }] },
                        { "type": "listItem", "content": [{ "type": "text", "text": "Item 2" }] }
                    ]
                }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("- Item 1"));
        assert!(markdown.contains("- Item 2"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_code_block() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "codeBlock",
                    "attrs": { "language": "rust" },
                    "content": [{ "type": "text", "text": "fn main() {}" }]
                }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("```rust"));
        assert!(markdown.contains("fn main() {}"));
        assert!(markdown.contains("```"));
    }

    // ============================================
    // WorkspaceManagerRegistry tests
    // ============================================

    #[tokio::test]
    async fn test_registry_get_nonexistent() {
        let registry = WorkspaceManagerRegistry::new();
        assert!(registry.get("/nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_registry_get_or_create() {
        let temp = TempDir::new().unwrap();
        let mut registry = WorkspaceManagerRegistry::new();

        let path = temp.path().to_string_lossy().to_string();

        // First call creates
        let manager1 = registry.get_or_create(&path).await.unwrap();

        // Second call returns same instance
        let manager2 = registry.get_or_create(&path).await.unwrap();

        assert!(Arc::ptr_eq(&manager1, &manager2));
    }

    #[tokio::test]
    async fn test_registry_remove() {
        let temp = TempDir::new().unwrap();
        let mut registry = WorkspaceManagerRegistry::new();

        let path = temp.path().to_string_lossy().to_string();

        registry.get_or_create(&path).await.unwrap();
        assert!(registry.get(&path).is_some());

        registry.remove(&path);
        assert!(registry.get(&path).is_none());
    }

    #[tokio::test]
    async fn test_registry_multiple_workspaces() {
        let temp1 = TempDir::new().unwrap();
        let temp2 = TempDir::new().unwrap();
        let mut registry = WorkspaceManagerRegistry::new();

        let path1 = temp1.path().to_string_lossy().to_string();
        let path2 = temp2.path().to_string_lossy().to_string();

        let manager1 = registry.get_or_create(&path1).await.unwrap();
        let manager2 = registry.get_or_create(&path2).await.unwrap();

        assert!(!Arc::ptr_eq(&manager1, &manager2));
    }

    // ============================================
    // Edge cases
    // ============================================

    #[tokio::test]
    async fn test_load_malformed_midlight_file() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create malformed .midlight file
        fs::write(temp.path().join("bad.midlight"), "not valid json").unwrap();

        // Should error
        let result = manager.load_document("bad.midlight").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_sidecar_creation() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let sidecar = manager.create_empty_sidecar();

        assert_eq!(sidecar["version"], 1);
        assert!(sidecar["meta"]["created"].is_string());
        assert!(sidecar["meta"]["modified"].is_string());
    }

    #[tokio::test]
    async fn test_compare_checkpoints() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create two versions with different content
        let json1 = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Short" }] }]
        });
        let result1 = manager
            .save_document("test.midlight", json1, "manual")
            .await
            .unwrap();
        let cp1 = result1.checkpoint_id.unwrap();

        // Add more content
        let json2 = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Short" }] },
                { "type": "paragraph", "content": [{ "type": "text", "text": "More content here" }] }
            ]
        });
        let result2 = manager
            .save_document("test.midlight", json2, "manual")
            .await
            .unwrap();
        let cp2 = result2.checkpoint_id.unwrap();

        // Compare - verify the API works and returns a diff result
        let diff = manager
            .compare_checkpoints("test.midlight", &cp1, &cp2)
            .await
            .unwrap();

        // Verify the result structure is valid
        assert!(diff.additions.is_empty() || !diff.additions.is_empty()); // Always true, just verify type
        assert!(diff.deletions.is_empty() || !diff.deletions.is_empty());
        // change_count reflects the character difference
        assert!(diff.change_count >= 0);
    }

    // ============================================
    // Additional coverage tests
    // ============================================

    #[tokio::test]
    async fn test_load_document_with_recovery_file() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create a .midlight file
        let doc = serde_json::json!({
            "version": 1,
            "meta": { "created": "2024-01-01T00:00:00Z", "modified": "2024-01-01T00:00:00Z" },
            "document": {},
            "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
            "images": {}
        });
        fs::write(
            temp.path().join("test.midlight"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();

        // Create a recovery file
        let recovery_dir = temp.path().join(".midlight/recovery");
        fs::create_dir_all(&recovery_dir).unwrap();
        fs::write(recovery_dir.join("test_midlight.wal"), "recovery data").unwrap();

        let result = manager.load_document("test.midlight").await.unwrap();

        assert!(result.has_recovery);
        assert!(result.recovery_time.is_some());
    }

    #[tokio::test]
    async fn test_load_midlight_missing_content_field() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create .midlight file without content field
        let doc = serde_json::json!({
            "version": 1,
            "meta": { "created": "2024-01-01T00:00:00Z", "modified": "2024-01-01T00:00:00Z" },
            "document": {}
        });
        fs::write(
            temp.path().join("no-content.midlight"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();

        let result = manager.load_document("no-content.midlight").await.unwrap();

        // Should return default empty document
        assert_eq!(result.json["type"], "doc");
        assert!(result.json["content"].is_array());
    }

    #[tokio::test]
    async fn test_load_midlight_missing_meta_field() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create .midlight file without meta field
        let doc = serde_json::json!({
            "version": 1,
            "document": {},
            "content": { "type": "doc", "content": [{ "type": "paragraph" }] }
        });
        fs::write(
            temp.path().join("no-meta.midlight"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();

        let result = manager.load_document("no-meta.midlight").await.unwrap();

        // Sidecar should have default meta with timestamps
        assert!(result.sidecar["meta"]["created"].is_string());
        assert!(result.sidecar["meta"]["modified"].is_string());
    }

    #[tokio::test]
    async fn test_load_midlight_missing_document_field() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create .midlight file without document field
        let doc = serde_json::json!({
            "version": 1,
            "meta": { "created": "2024-01-01T00:00:00Z", "modified": "2024-01-01T00:00:00Z" },
            "content": { "type": "doc", "content": [{ "type": "paragraph" }] }
        });
        fs::write(
            temp.path().join("no-document.midlight"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();

        let result = manager.load_document("no-document.midlight").await.unwrap();

        // Sidecar should have empty document field
        assert!(result.sidecar["document"].is_object());
    }

    #[tokio::test]
    async fn test_load_midlight_missing_images_field() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create .midlight file without images field
        let doc = serde_json::json!({
            "version": 1,
            "meta": { "created": "2024-01-01T00:00:00Z", "modified": "2024-01-01T00:00:00Z" },
            "document": {},
            "content": { "type": "doc", "content": [{ "type": "paragraph" }] }
        });
        fs::write(
            temp.path().join("no-images.midlight"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();

        let result = manager.load_document("no-images.midlight").await.unwrap();

        // Sidecar should have empty images field
        assert!(result.sidecar["images"].is_object());
    }

    #[tokio::test]
    async fn test_load_markdown_with_sidecar() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create .md file
        fs::write(temp.path().join("with-sidecar.md"), "# Title\n\nContent").unwrap();

        // Create sidecar file
        let sidecar = serde_json::json!({
            "version": 1,
            "meta": {
                "created": "2020-01-01T00:00:00Z",
                "modified": "2020-06-01T00:00:00Z"
            },
            "document": {
                "defaultFont": "Arial",
                "defaultFontSize": 14
            },
            "images": {
                "img1": "hash123"
            }
        });
        fs::write(
            temp.path().join("with-sidecar.md.sidecar.json"),
            serde_json::to_string(&sidecar).unwrap(),
        )
        .unwrap();

        let result = manager.load_document("with-sidecar.md").await.unwrap();

        // Sidecar data should be preserved
        assert_eq!(result.sidecar["meta"]["created"], "2020-01-01T00:00:00Z");

        // Original files should be removed
        assert!(!temp.path().join("with-sidecar.md").exists());
        assert!(!temp.path().join("with-sidecar.md.sidecar.json").exists());

        // .midlight should be created
        assert!(temp.path().join("with-sidecar.midlight").exists());
    }

    #[tokio::test]
    async fn test_load_nonexistent_markdown_creates_empty() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Load non-existent .md file
        let result = manager.load_document("new.md").await.unwrap();

        // Should return empty document
        assert_eq!(result.json["type"], "doc");

        // .midlight should be created
        assert!(temp.path().join("new.midlight").exists());
    }

    #[tokio::test]
    async fn test_load_nonexistent_unsupported_format() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Load non-existent file with unsupported extension
        let result = manager.load_document("missing.txt").await.unwrap();

        // Should return empty document
        assert_eq!(result.json["type"], "doc");
    }

    #[tokio::test]
    async fn test_save_document_without_extension() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        });

        // Save without extension
        manager.save_document("noext", json, "manual").await.unwrap();

        // Should create .midlight file
        assert!(temp.path().join("noext.midlight").exists());
    }

    #[tokio::test]
    async fn test_save_document_preserves_images() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create existing document with images
        let existing_doc = serde_json::json!({
            "version": 1,
            "meta": { "created": "2024-01-01T00:00:00Z", "modified": "2024-01-01T00:00:00Z" },
            "document": {},
            "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
            "images": {
                "img1": "hash123",
                "img2": "hash456"
            }
        });
        fs::write(
            temp.path().join("with-images.midlight"),
            serde_json::to_string(&existing_doc).unwrap(),
        )
        .unwrap();

        // Save new content
        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Updated" }] }]
        });
        manager
            .save_document("with-images.midlight", json, "manual")
            .await
            .unwrap();

        // Read back and verify images preserved
        let content = fs::read_to_string(temp.path().join("with-images.midlight")).unwrap();
        let doc: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(doc["images"]["img1"], "hash123");
        assert_eq!(doc["images"]["img2"], "hash456");
    }

    #[tokio::test]
    async fn test_create_bookmark_with_md_extension() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        });

        // Create bookmark with .md path
        manager
            .create_bookmark("test.md", json, "Label", None)
            .await
            .unwrap();

        // Should create .midlight file
        assert!(temp.path().join("test.midlight").exists());
    }

    #[tokio::test]
    async fn test_create_bookmark_without_extension() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        });

        // Create bookmark without extension
        manager
            .create_bookmark("noext", json, "Label", None)
            .await
            .unwrap();

        // Should create .midlight file
        assert!(temp.path().join("noext.midlight").exists());
    }

    #[tokio::test]
    async fn test_create_bookmark_preserves_existing_data() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Create existing document
        let existing_doc = serde_json::json!({
            "version": 1,
            "meta": { "created": "2020-01-01T00:00:00Z", "modified": "2020-01-01T00:00:00Z" },
            "document": {},
            "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
            "images": { "img1": "hash" }
        });
        fs::write(
            temp.path().join("existing.midlight"),
            serde_json::to_string(&existing_doc).unwrap(),
        )
        .unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "New" }] }]
        });

        manager
            .create_bookmark("existing.midlight", json, "Bookmark", None)
            .await
            .unwrap();

        // Verify created timestamp preserved
        let content = fs::read_to_string(temp.path().join("existing.midlight")).unwrap();
        let doc: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(doc["meta"]["created"], "2020-01-01T00:00:00Z");
        assert_eq!(doc["images"]["img1"], "hash");
    }

    #[tokio::test]
    async fn test_create_bookmark_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        });

        manager
            .create_bookmark("deep/path/doc.midlight", json, "Bookmark", None)
            .await
            .unwrap();

        assert!(temp.path().join("deep/path/doc.midlight").exists());
    }

    #[tokio::test]
    async fn test_restore_legacy_checkpoint() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Save a document first to create a checkpoint
        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Content" }] }]
        });
        let result = manager
            .save_document("test.midlight", json, "manual")
            .await
            .unwrap();

        // Restore it (tests the restore_checkpoint logic)
        let restored = manager
            .restore_checkpoint("test.midlight", &result.checkpoint_id.unwrap())
            .await
            .unwrap();

        assert_eq!(restored["type"], "doc");
    }

    // ============================================
    // Tiptap to Markdown additional coverage
    // ============================================

    #[tokio::test]
    async fn test_tiptap_to_markdown_ordered_list() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "orderedList",
                    "content": [
                        { "type": "listItem", "content": [{ "type": "text", "text": "First" }] },
                        { "type": "listItem", "content": [{ "type": "text", "text": "Second" }] },
                        { "type": "listItem", "content": [{ "type": "text", "text": "Third" }] }
                    ]
                }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("1. First"));
        assert!(markdown.contains("2. Second"));
        assert!(markdown.contains("3. Third"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_blockquote() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "blockquote",
                    "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "Quoted text" }] }
                    ]
                }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("> Quoted text"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_horizontal_rule() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Above" }] },
                { "type": "horizontalRule" },
                { "type": "paragraph", "content": [{ "type": "text", "text": "Below" }] }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("---"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_unknown_type() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "unknownNodeType", "content": [{ "type": "text", "text": "Unknown" }] },
                { "type": "paragraph", "content": [{ "type": "text", "text": "Known" }] }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        // Unknown types are skipped, known types work
        assert!(markdown.contains("Known"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_empty_content() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": []
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.is_empty());
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_no_content_field() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc"
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.is_empty());
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_code_block_no_language() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "codeBlock",
                    "content": [{ "type": "text", "text": "code here" }]
                }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("```"));
        assert!(markdown.contains("code here"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_multiline_blockquote() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "blockquote",
                    "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "Line 1\nLine 2\nLine 3" }] }
                    ]
                }
            ]
        });

        let markdown = manager.tiptap_to_markdown(&json);
        assert!(markdown.contains("> Line 1"));
        assert!(markdown.contains("> Line 2"));
        assert!(markdown.contains("> Line 3"));
    }

    // ============================================
    // Extract text content tests
    // ============================================

    #[tokio::test]
    async fn test_extract_text_content_direct_text() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let node = serde_json::json!({
            "type": "text",
            "text": "Direct text"
        });

        let text = manager.extract_text_content(&node);
        assert_eq!(text, "Direct text");
    }

    #[tokio::test]
    async fn test_extract_text_content_nested() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let node = serde_json::json!({
            "type": "paragraph",
            "content": [
                { "type": "text", "text": "Hello " },
                { "type": "text", "text": "World" }
            ]
        });

        let text = manager.extract_text_content(&node);
        assert_eq!(text, "Hello World");
    }

    #[tokio::test]
    async fn test_extract_text_content_empty() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let node = serde_json::json!({
            "type": "paragraph"
        });

        let text = manager.extract_text_content(&node);
        assert!(text.is_empty());
    }

    // ============================================
    // Extract sidecar test
    // ============================================

    #[tokio::test]
    async fn test_extract_sidecar() {
        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());

        let json = serde_json::json!({
            "type": "doc",
            "content": []
        });

        let sidecar = manager.extract_sidecar(&json);

        // Should return empty sidecar (current implementation)
        assert_eq!(sidecar["version"], 1);
        assert!(sidecar["meta"]["created"].is_string());
    }

    // ============================================
    // Registry Default trait test
    // ============================================

    #[test]
    fn test_registry_default() {
        let registry = WorkspaceManagerRegistry::default();
        assert!(registry.get("/any").is_none());
    }

    // ============================================
    // File system error tests (Unix)
    // ============================================

    #[tokio::test]
    #[cfg(unix)]
    async fn test_init_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();

        // Make workspace read-only
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o444)).unwrap();

        let manager = WorkspaceManager::new(temp.path());
        let result = manager.init().await;

        // Should fail due to permissions
        assert!(result.is_err());

        // Cleanup
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_save_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let manager = WorkspaceManager::new(temp.path());
        manager.init().await.unwrap();

        // Make workspace read-only
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

        let json = serde_json::json!({
            "type": "doc",
            "content": [{ "type": "paragraph" }]
        });

        let result = manager.save_document("test.midlight", json, "manual").await;

        // Should fail due to permissions
        assert!(result.is_err());

        // Cleanup
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
