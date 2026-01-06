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
                .map(|t| {
                    chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
                })
        } else {
            None
        };

        // Handle based on file extension
        if file_path.ends_with(".midlight") {
            // Native .midlight format - read directly
            self.load_midlight_document(&full_path, has_recovery, recovery_time)
        } else if file_path.ends_with(".md") {
            // Legacy .md format - migrate to .midlight
            self.load_and_migrate_markdown(&full_path, file_path, has_recovery, recovery_time).await
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
        let document = midlight_doc.get("document").cloned().unwrap_or_else(|| {
            serde_json::json!({})
        });
        let images = midlight_doc.get("images").cloned().unwrap_or_else(|| {
            serde_json::json!({})
        });

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

        let meta = sidecar.get("meta").cloned().unwrap_or_else(|| {
            serde_json::json!({ "created": now, "modified": now })
        });
        let document = sidecar.get("document").cloned().unwrap_or_else(|| {
            serde_json::json!({ "defaultFont": "Merriweather", "defaultFontSize": 16 })
        });
        let images = sidecar.get("images").cloned().unwrap_or_else(|| {
            serde_json::json!({})
        });

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
            let existing = fs::read_to_string(&full_path).ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok());
            let created = existing.as_ref()
                .and_then(|d| d.get("meta"))
                .and_then(|m| m.get("created"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            let images = existing.as_ref()
                .and_then(|d| d.get("images"))
                .cloned();
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
            .create_checkpoint(&midlight_path, &content_for_checkpoint, sidecar_placeholder, trigger, None, None)
            .await?;

        // Clear recovery file
        let recovery_path = self.midlight_dir.join("recovery").join(format!(
            "{}.wal",
            midlight_path.replace(['/', '\\'], "__").replace('.', "_")
        ));
        let _ = fs::remove_file(recovery_path);

        tracing::debug!("Saved document: {} (checkpoint: {})", midlight_path, &checkpoint.id[..8]);

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
            let existing = fs::read_to_string(&full_path).ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok());
            let created = existing.as_ref()
                .and_then(|d| d.get("meta"))
                .and_then(|m| m.get("created"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            let images = existing.as_ref()
                .and_then(|d| d.get("images"))
                .cloned();
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
            change_count: (cp_b.stats.char_count as i32 - cp_a.stats.char_count as i32).unsigned_abs(),
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
        self.managers.insert(workspace_root.to_string(), manager.clone());

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
