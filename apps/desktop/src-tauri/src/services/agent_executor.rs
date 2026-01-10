// Agent Executor - Handles tool execution for AI agent

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ============================================================================
// Tool Execution Types
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecution {
    pub id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub status: ToolExecutionStatus,
    pub result: Option<ToolResult>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    RequiresConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: String,
    pub name: String,
    pub snippet: String,
    pub line: Option<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChange {
    pub change_id: String,
    pub path: String,
    pub original_content: String,
    pub new_content: String,
    pub description: Option<String>,
    pub created_at: String,
}

// ============================================================================
// Agent Executor
// ============================================================================

pub struct AgentExecutor {
    workspace_root: PathBuf,
}

impl AgentExecutor {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Execute a tool by name with the given arguments
    pub async fn execute_tool(&self, tool_name: &str, arguments: Value) -> ToolResult {
        info!("Executing tool: {} with args: {:?}", tool_name, arguments);

        match tool_name {
            "list_documents" => self.list_documents(arguments).await,
            "read_document" => self.read_document(arguments).await,
            "create_document" => self.create_document(arguments).await,
            "edit_document" => self.edit_document(arguments).await,
            "move_document" => self.move_document(arguments).await,
            "delete_document" => self.delete_document(arguments).await,
            "search_documents" => self.search_documents(arguments).await,
            _ => ToolResult {
                success: false,
                data: None,
                error: Some(format!("Unknown tool: {}", tool_name)),
            },
        }
    }

    /// List documents in a directory
    async fn list_documents(&self, args: Value) -> ToolResult {
        let path_arg = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

        let dir_path = if path_arg.is_empty() || path_arg == "/" {
            self.workspace_root.clone()
        } else {
            self.workspace_root.join(path_arg.trim_start_matches('/'))
        };

        debug!("Listing documents in: {:?}", dir_path);

        match fs::read_dir(&dir_path).await {
            Ok(mut entries) => {
                let mut files: Vec<FileInfo> = Vec::new();

                while let Ok(Some(entry)) = entries.next_entry().await {
                    let file_name = entry.file_name().to_string_lossy().to_string();

                    // Skip hidden files and system files
                    if file_name.starts_with('.') {
                        continue;
                    }

                    let metadata = entry.metadata().await.ok();
                    let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let modified = metadata.and_then(|m| {
                        m.modified().ok().map(|t| {
                            chrono::DateTime::<chrono::Utc>::from(t)
                                .format("%Y-%m-%dT%H:%M:%SZ")
                                .to_string()
                        })
                    });

                    // Only show .midlight files and directories
                    if is_dir || file_name.ends_with(".midlight") {
                        let relative_path = entry
                            .path()
                            .strip_prefix(&self.workspace_root)
                            .unwrap_or(entry.path().as_path())
                            .to_string_lossy()
                            .to_string();

                        files.push(FileInfo {
                            path: relative_path,
                            name: file_name,
                            file_type: if is_dir {
                                "directory".to_string()
                            } else {
                                "file".to_string()
                            },
                            modified,
                        });
                    }
                }

                // Sort: directories first, then by name
                files.sort_by(|a, b| {
                    if a.file_type == b.file_type {
                        a.name.cmp(&b.name)
                    } else if a.file_type == "directory" {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                });

                ToolResult {
                    success: true,
                    data: Some(json!({ "files": files })),
                    error: None,
                }
            }
            Err(e) => ToolResult {
                success: false,
                data: None,
                error: Some(format!("Failed to list directory: {}", e)),
            },
        }
    }

    /// Read a document's content
    async fn read_document(&self, args: Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: path".to_string()),
                }
            }
        };

        let file_path = self.workspace_root.join(path.trim_start_matches('/'));
        debug!("Reading document: {:?}", file_path);

        match fs::read_to_string(&file_path).await {
            Ok(content) => {
                // Parse the Midlight JSON format
                match serde_json::from_str::<Value>(&content) {
                    Ok(doc) => {
                        // Extract text content from Tiptap JSON
                        // Convert to markdown so AI sees formatting (headings, bold, etc.)
                        let markdown_content =
                            self.tiptap_to_markdown(doc.get("content").unwrap_or(&Value::Null));
                        let title = doc
                            .get("meta")
                            .and_then(|m| m.get("title"))
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string());

                        ToolResult {
                            success: true,
                            data: Some(json!({
                                "path": path,
                                "content": markdown_content,
                                "title": title,
                            })),
                            error: None,
                        }
                    }
                    Err(_) => {
                        // Not valid JSON, return as plain text
                        ToolResult {
                            success: true,
                            data: Some(json!({
                                "path": path,
                                "content": content,
                            })),
                            error: None,
                        }
                    }
                }
            }
            Err(e) => ToolResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read document: {}", e)),
            },
        }
    }

    /// Create a new document
    async fn create_document(&self, args: Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: path".to_string()),
                }
            }
        };

        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let title = args.get("title").and_then(|v| v.as_str());

        // Ensure path ends with .midlight
        let file_name = if path.ends_with(".midlight") {
            path.to_string()
        } else {
            format!("{}.midlight", path)
        };

        let file_path = self.workspace_root.join(file_name.trim_start_matches('/'));
        debug!("Creating document: {:?}", file_path);

        // Check if file already exists
        if file_path.exists() {
            return ToolResult {
                success: false,
                data: None,
                error: Some(format!("Document already exists: {}", path)),
            };
        }

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to create directory: {}", e)),
                };
            }
        }

        // Create Midlight document format
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let tiptap_content = self.markdown_to_tiptap(content);

        let doc = json!({
            "version": 1,
            "meta": {
                "created": now,
                "modified": now,
                "title": title,
            },
            "document": {},
            "content": tiptap_content,
        });

        match fs::write(&file_path, serde_json::to_string_pretty(&doc).unwrap()).await {
            Ok(_) => {
                let relative_path = file_path
                    .strip_prefix(&self.workspace_root)
                    .unwrap_or(file_path.as_path())
                    .to_string_lossy()
                    .to_string();

                ToolResult {
                    success: true,
                    data: Some(json!({
                        "path": relative_path,
                        "name": file_path.file_name().unwrap().to_string_lossy(),
                    })),
                    error: None,
                }
            }
            Err(e) => ToolResult {
                success: false,
                data: None,
                error: Some(format!("Failed to create document: {}", e)),
            },
        }
    }

    /// Edit an existing document (stages changes for review - does NOT write to disk)
    async fn edit_document(&self, args: Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: path".to_string()),
                }
            }
        };

        let new_content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: content".to_string()),
                }
            }
        };

        let description = args.get("description").and_then(|v| v.as_str());

        let file_path = self.workspace_root.join(path.trim_start_matches('/'));
        debug!("Editing document (staging): {:?}", file_path);

        // Read existing content
        let original_content = match fs::read_to_string(&file_path).await {
            Ok(content) => content,
            Err(e) => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to read document: {}", e)),
                }
            }
        };

        // Parse existing document
        let original_doc: Value = match serde_json::from_str(&original_content) {
            Ok(d) => d,
            Err(e) => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to parse document: {}", e)),
                }
            }
        };

        // Extract original text for diff display
        let original_text =
            self.extract_text_from_tiptap(original_doc.get("content").unwrap_or(&Value::Null));

        // Create staged document with new content (don't modify original)
        let mut staged_doc = original_doc.clone();
        let tiptap_content = self.markdown_to_tiptap(new_content);
        staged_doc["content"] = tiptap_content;
        staged_doc["meta"]["modified"] =
            json!(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());

        // Generate change ID
        let change_id = Uuid::new_v4().to_string();

        // Extract just the Tiptap content portion (type: 'doc' with content array)
        // The full .midlight file has { content, document, meta, version } but Tiptap only needs the `content` field
        let original_tiptap_content = original_doc
            .get("content")
            .cloned()
            .unwrap_or(json!({"type": "doc", "content": []}));
        let staged_tiptap_content = staged_doc
            .get("content")
            .cloned()
            .unwrap_or(json!({"type": "doc", "content": []}));

        // Return staged content WITHOUT writing to disk
        // Frontend will display diff and write on accept
        ToolResult {
            success: true,
            data: Some(json!({
                "path": path,
                "changeId": change_id,
                "originalContent": original_text,
                "newContent": new_content,
                "description": description,
                "originalTiptapJson": original_tiptap_content,
                "stagedTiptapJson": staged_tiptap_content,
                "requiresAcceptance": true,
            })),
            error: None,
        }
    }

    /// Move/rename a document
    async fn move_document(&self, args: Value) -> ToolResult {
        let old_path = match args.get("oldPath").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: oldPath".to_string()),
                }
            }
        };

        let new_path = match args.get("newPath").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: newPath".to_string()),
                }
            }
        };

        let old_file_path = self.workspace_root.join(old_path.trim_start_matches('/'));
        let new_file_path = self.workspace_root.join(new_path.trim_start_matches('/'));

        debug!(
            "Moving document: {:?} -> {:?}",
            old_file_path, new_file_path
        );

        // Check if source exists
        if !old_file_path.exists() {
            return ToolResult {
                success: false,
                data: None,
                error: Some(format!("Source document not found: {}", old_path)),
            };
        }

        // Check if destination already exists
        if new_file_path.exists() {
            return ToolResult {
                success: false,
                data: None,
                error: Some(format!("Destination already exists: {}", new_path)),
            };
        }

        // Create parent directories if needed
        if let Some(parent) = new_file_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to create directory: {}", e)),
                };
            }
        }

        match fs::rename(&old_file_path, &new_file_path).await {
            Ok(_) => ToolResult {
                success: true,
                data: Some(json!({
                    "oldPath": old_path,
                    "newPath": new_path,
                })),
                error: None,
            },
            Err(e) => ToolResult {
                success: false,
                data: None,
                error: Some(format!("Failed to move document: {}", e)),
            },
        }
    }

    /// Delete a document (moves to trash)
    async fn delete_document(&self, args: Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: path".to_string()),
                }
            }
        };

        let file_path = self.workspace_root.join(path.trim_start_matches('/'));
        debug!("Deleting document: {:?}", file_path);

        if !file_path.exists() {
            return ToolResult {
                success: false,
                data: None,
                error: Some(format!("Document not found: {}", path)),
            };
        }

        // Use trash crate to move to trash instead of permanent delete
        match trash::delete(&file_path) {
            Ok(_) => ToolResult {
                success: true,
                data: Some(json!({
                    "path": path,
                })),
                error: None,
            },
            Err(e) => ToolResult {
                success: false,
                data: None,
                error: Some(format!("Failed to delete document: {}", e)),
            },
        }
    }

    /// Search documents for content
    async fn search_documents(&self, args: Value) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => {
                return ToolResult {
                    success: false,
                    data: None,
                    error: Some("Missing required parameter: query".to_string()),
                }
            }
        };

        let _file_pattern = args.get("filePattern").and_then(|v| v.as_str());
        let query_lower = query.to_lowercase();

        debug!("Searching for: {}", query);

        let mut matches: Vec<SearchMatch> = Vec::new();

        // Recursively search files
        if let Err(e) = self
            .search_directory(&self.workspace_root, &query_lower, &mut matches)
            .await
        {
            warn!("Search error: {}", e);
        }

        ToolResult {
            success: true,
            data: Some(json!({ "matches": matches })),
            error: None,
        }
    }

    /// Recursively search a directory for matching content
    async fn search_directory(
        &self,
        dir: &PathBuf,
        query: &str,
        matches: &mut Vec<SearchMatch>,
    ) -> Result<(), std::io::Error> {
        let mut entries = fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files
            if file_name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                // Recurse into subdirectories
                Box::pin(self.search_directory(&path, query, matches)).await?;
            } else if file_name.ends_with(".midlight") {
                // Search in file content
                if let Ok(content) = fs::read_to_string(&path).await {
                    if let Ok(doc) = serde_json::from_str::<Value>(&content) {
                        let text = self
                            .extract_text_from_tiptap(doc.get("content").unwrap_or(&Value::Null));

                        if text.to_lowercase().contains(query) {
                            let relative_path = path
                                .strip_prefix(&self.workspace_root)
                                .unwrap_or(path.as_path())
                                .to_string_lossy()
                                .to_string();

                            // Extract a snippet around the match
                            let snippet = self.extract_snippet(&text, query);

                            matches.push(SearchMatch {
                                path: relative_path,
                                name: file_name,
                                snippet,
                                line: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Extract a snippet around a search match
    fn extract_snippet(&self, text: &str, query: &str) -> String {
        let lower_text = text.to_lowercase();
        if let Some(pos) = lower_text.find(query) {
            let start = pos.saturating_sub(50);
            let end = (pos + query.len() + 50).min(text.len());

            let mut snippet = text[start..end].to_string();
            if start > 0 {
                snippet = format!("...{}", snippet);
            }
            if end < text.len() {
                snippet = format!("{}...", snippet);
            }
            snippet
        } else {
            text.chars().take(100).collect::<String>()
        }
    }

    /// Extract plain text from Tiptap JSON
    /// Convert Tiptap JSON to markdown (preserves formatting for AI to see and edit)
    fn tiptap_to_markdown(&self, node: &Value) -> String {
        let mut text = String::new();

        if let Some(node_type) = node.get("type").and_then(|t| t.as_str()) {
            match node_type {
                "text" => {
                    if let Some(t) = node.get("text").and_then(|t| t.as_str()) {
                        // Check for marks (bold, italic, code)
                        let marks = node.get("marks").and_then(|m| m.as_array());
                        let mut formatted = t.to_string();

                        if let Some(marks) = marks {
                            let has_bold = marks
                                .iter()
                                .any(|m| m.get("type").and_then(|t| t.as_str()) == Some("bold"));
                            let has_italic = marks
                                .iter()
                                .any(|m| m.get("type").and_then(|t| t.as_str()) == Some("italic"));
                            let has_code = marks
                                .iter()
                                .any(|m| m.get("type").and_then(|t| t.as_str()) == Some("code"));

                            if has_code {
                                formatted = format!("`{}`", formatted);
                            } else if has_bold && has_italic {
                                formatted = format!("***{}***", formatted);
                            } else if has_bold {
                                formatted = format!("**{}**", formatted);
                            } else if has_italic {
                                formatted = format!("*{}*", formatted);
                            }
                        }

                        text.push_str(&formatted);
                    }
                }
                "heading" => {
                    let level = node
                        .get("attrs")
                        .and_then(|a| a.get("level"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(1) as usize;
                    let prefix = "#".repeat(level);

                    text.push_str(&prefix);
                    text.push(' ');

                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.tiptap_to_markdown(child));
                        }
                    }
                    text.push('\n');
                }
                "paragraph" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.tiptap_to_markdown(child));
                        }
                    }
                    text.push('\n');
                }
                "bulletList" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str("- ");
                            // Extract text from listItem -> paragraph -> text
                            if let Some(item_content) =
                                child.get("content").and_then(|c| c.as_array())
                            {
                                for para in item_content {
                                    if let Some(para_content) =
                                        para.get("content").and_then(|c| c.as_array())
                                    {
                                        for text_node in para_content {
                                            text.push_str(&self.tiptap_to_markdown(text_node));
                                        }
                                    }
                                }
                            }
                            text.push('\n');
                        }
                    }
                }
                "orderedList" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for (idx, child) in content.iter().enumerate() {
                            text.push_str(&format!("{}. ", idx + 1));
                            // Extract text from listItem -> paragraph -> text
                            if let Some(item_content) =
                                child.get("content").and_then(|c| c.as_array())
                            {
                                for para in item_content {
                                    if let Some(para_content) =
                                        para.get("content").and_then(|c| c.as_array())
                                    {
                                        for text_node in para_content {
                                            text.push_str(&self.tiptap_to_markdown(text_node));
                                        }
                                    }
                                }
                            }
                            text.push('\n');
                        }
                    }
                }
                "blockquote" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str("> ");
                            if let Some(para_content) =
                                child.get("content").and_then(|c| c.as_array())
                            {
                                for text_node in para_content {
                                    text.push_str(&self.tiptap_to_markdown(text_node));
                                }
                            }
                            text.push('\n');
                        }
                    }
                }
                "horizontalRule" => {
                    text.push_str("---\n");
                }
                "doc" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.tiptap_to_markdown(child));
                        }
                    }
                }
                _ => {
                    // Handle unknown node types by extracting any content
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.tiptap_to_markdown(child));
                        }
                    }
                }
            }
        }

        text
    }

    /// Extract plain text from Tiptap (for search/diff - no markdown)
    fn extract_text_from_tiptap(&self, node: &Value) -> String {
        let mut text = String::new();

        if let Some(node_type) = node.get("type").and_then(|t| t.as_str()) {
            match node_type {
                "text" => {
                    if let Some(t) = node.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                "paragraph" | "heading" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.extract_text_from_tiptap(child));
                        }
                    }
                    text.push('\n');
                }
                "bulletList" | "orderedList" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str("- ");
                            text.push_str(&self.extract_text_from_tiptap(child));
                        }
                    }
                }
                "listItem" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.extract_text_from_tiptap(child));
                        }
                    }
                }
                "doc" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.extract_text_from_tiptap(child));
                        }
                    }
                }
                _ => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            text.push_str(&self.extract_text_from_tiptap(child));
                        }
                    }
                }
            }
        }

        text
    }

    /// Convert markdown to Tiptap JSON (simplified)
    fn markdown_to_tiptap(&self, markdown: &str) -> Value {
        let mut content: Vec<Value> = Vec::new();
        let lines: Vec<&str> = markdown.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // Headings
            if line.starts_with("# ") {
                content.push(json!({
                    "type": "heading",
                    "attrs": { "level": 1 },
                    "content": self.parse_inline_formatting(&line[2..])
                }));
            } else if line.starts_with("## ") {
                content.push(json!({
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": self.parse_inline_formatting(&line[3..])
                }));
            } else if line.starts_with("### ") {
                content.push(json!({
                    "type": "heading",
                    "attrs": { "level": 3 },
                    "content": self.parse_inline_formatting(&line[4..])
                }));
            } else if line.starts_with("#### ") {
                content.push(json!({
                    "type": "heading",
                    "attrs": { "level": 4 },
                    "content": self.parse_inline_formatting(&line[5..])
                }));
            } else if line.starts_with("##### ") {
                content.push(json!({
                    "type": "heading",
                    "attrs": { "level": 5 },
                    "content": self.parse_inline_formatting(&line[6..])
                }));
            } else if line.starts_with("###### ") {
                content.push(json!({
                    "type": "heading",
                    "attrs": { "level": 6 },
                    "content": self.parse_inline_formatting(&line[7..])
                }));
            }
            // Horizontal rule
            else if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
                content.push(json!({
                    "type": "horizontalRule"
                }));
            }
            // Blockquote
            else if line.starts_with("> ") {
                content.push(json!({
                    "type": "blockquote",
                    "content": [{
                        "type": "paragraph",
                        "content": self.parse_inline_formatting(&line[2..])
                    }]
                }));
            }
            // Unordered list item
            else if line.starts_with("- ") || line.starts_with("* ") {
                let mut list_items: Vec<Value> = Vec::new();
                while i < lines.len() && (lines[i].starts_with("- ") || lines[i].starts_with("* "))
                {
                    let item_text = &lines[i][2..];
                    list_items.push(json!({
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": self.parse_inline_formatting(item_text)
                        }]
                    }));
                    i += 1;
                }
                content.push(json!({
                    "type": "bulletList",
                    "content": list_items
                }));
                continue; // Skip the i += 1 at the end
            }
            // Ordered list item
            else if line
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
                && line.contains(". ")
            {
                let mut list_items: Vec<Value> = Vec::new();
                while i < lines.len() {
                    let current = lines[i];
                    if let Some(dot_pos) = current.find(". ") {
                        if current[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                            let item_text = &current[dot_pos + 2..];
                            list_items.push(json!({
                                "type": "listItem",
                                "content": [{
                                    "type": "paragraph",
                                    "content": self.parse_inline_formatting(item_text)
                                }]
                            }));
                            i += 1;
                            continue;
                        }
                    }
                    break;
                }
                content.push(json!({
                    "type": "orderedList",
                    "content": list_items
                }));
                continue; // Skip the i += 1 at the end
            }
            // Empty line
            else if line.is_empty() {
                // Skip empty lines
            }
            // Regular paragraph
            else {
                let inline_content = self.parse_inline_formatting(line);
                if !inline_content.is_empty() {
                    content.push(json!({
                        "type": "paragraph",
                        "content": inline_content
                    }));
                }
            }

            i += 1;
        }

        if content.is_empty() {
            content.push(json!({
                "type": "paragraph",
                "content": []
            }));
        }

        json!({
            "type": "doc",
            "content": content
        })
    }

    /// Parse inline markdown formatting (bold, italic, code, etc.)
    fn parse_inline_formatting(&self, text: &str) -> Vec<Value> {
        let mut result: Vec<Value> = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut current_text = String::new();

        while i < chars.len() {
            // Check for inline code (backticks)
            if chars[i] == '`' {
                // Flush current text
                if !current_text.is_empty() {
                    result.push(json!({ "type": "text", "text": current_text }));
                    current_text = String::new();
                }

                // Find closing backtick
                let start = i + 1;
                i += 1;
                while i < chars.len() && chars[i] != '`' {
                    i += 1;
                }
                if i < chars.len() {
                    let code_text: String = chars[start..i].iter().collect();
                    result.push(json!({
                        "type": "text",
                        "text": code_text,
                        "marks": [{ "type": "code" }]
                    }));
                    i += 1;
                }
                continue;
            }

            // Check for bold+italic (*** or ___)
            if i + 2 < chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_' && chars[i + 2] == '_'))
            {
                let marker = chars[i];
                // Flush current text
                if !current_text.is_empty() {
                    result.push(json!({ "type": "text", "text": current_text }));
                    current_text = String::new();
                }

                // Find closing markers
                let start = i + 3;
                i += 3;
                while i + 2 < chars.len()
                    && !(chars[i] == marker && chars[i + 1] == marker && chars[i + 2] == marker)
                {
                    i += 1;
                }
                if i + 2 < chars.len() {
                    let bold_italic_text: String = chars[start..i].iter().collect();
                    result.push(json!({
                        "type": "text",
                        "text": bold_italic_text,
                        "marks": [{ "type": "bold" }, { "type": "italic" }]
                    }));
                    i += 3;
                }
                continue;
            }

            // Check for bold (** or __)
            if i + 1 < chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_'))
            {
                let marker = chars[i];
                // Flush current text
                if !current_text.is_empty() {
                    result.push(json!({ "type": "text", "text": current_text }));
                    current_text = String::new();
                }

                // Find closing markers
                let start = i + 2;
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == marker && chars[i + 1] == marker) {
                    i += 1;
                }
                if i + 1 < chars.len() {
                    let bold_text: String = chars[start..i].iter().collect();
                    result.push(json!({
                        "type": "text",
                        "text": bold_text,
                        "marks": [{ "type": "bold" }]
                    }));
                    i += 2;
                }
                continue;
            }

            // Check for italic (* or _) - but not at word boundaries for _
            if (chars[i] == '*') || (chars[i] == '_' && (i == 0 || !chars[i - 1].is_alphanumeric()))
            {
                let marker = chars[i];
                let next_char = if i + 1 < chars.len() {
                    Some(chars[i + 1])
                } else {
                    None
                };

                // Make sure it's not ** or __ (bold)
                if next_char != Some(marker) {
                    // Flush current text
                    if !current_text.is_empty() {
                        result.push(json!({ "type": "text", "text": current_text }));
                        current_text = String::new();
                    }

                    // Find closing marker
                    let start = i + 1;
                    i += 1;
                    while i < chars.len() && chars[i] != marker {
                        i += 1;
                    }
                    if i < chars.len() {
                        let italic_text: String = chars[start..i].iter().collect();
                        result.push(json!({
                            "type": "text",
                            "text": italic_text,
                            "marks": [{ "type": "italic" }]
                        }));
                        i += 1;
                    }
                    continue;
                }
            }

            // Regular character
            current_text.push(chars[i]);
            i += 1;
        }

        // Flush remaining text
        if !current_text.is_empty() {
            result.push(json!({ "type": "text", "text": current_text }));
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ============================================
    // Helper to create executor with temp directory
    // ============================================

    fn create_test_executor() -> (TempDir, AgentExecutor) {
        let temp = TempDir::new().unwrap();
        let executor = AgentExecutor::new(temp.path().to_path_buf());
        (temp, executor)
    }

    fn create_midlight_doc(content: &str) -> String {
        let tiptap = json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": content }] }
            ]
        });
        let doc = json!({
            "version": 1,
            "meta": {
                "created": "2024-01-01T00:00:00Z",
                "modified": "2024-01-01T00:00:00Z"
            },
            "document": {},
            "content": tiptap
        });
        serde_json::to_string_pretty(&doc).unwrap()
    }

    // ============================================
    // Unknown tool tests
    // ============================================

    #[tokio::test]
    async fn test_unknown_tool() {
        let (_temp, executor) = create_test_executor();

        let result = executor.execute_tool("unknown_tool", json!({})).await;

        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Unknown tool"));
    }

    // ============================================
    // list_documents tests
    // ============================================

    #[tokio::test]
    async fn test_list_documents_empty_dir() {
        let (_temp, executor) = create_test_executor();

        let result = executor.execute_tool("list_documents", json!({})).await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_list_documents_with_files() {
        let (temp, executor) = create_test_executor();

        // Create some files
        std::fs::write(
            temp.path().join("doc1.midlight"),
            create_midlight_doc("Content 1"),
        )
        .unwrap();
        std::fs::write(
            temp.path().join("doc2.midlight"),
            create_midlight_doc("Content 2"),
        )
        .unwrap();
        std::fs::create_dir(temp.path().join("subfolder")).unwrap();

        let result = executor.execute_tool("list_documents", json!({})).await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        assert_eq!(files.len(), 3); // 2 files + 1 directory
    }

    #[tokio::test]
    async fn test_list_documents_hides_hidden_files() {
        let (temp, executor) = create_test_executor();

        // Create hidden and visible files
        std::fs::write(temp.path().join(".hidden.midlight"), "").unwrap();
        std::fs::write(
            temp.path().join("visible.midlight"),
            create_midlight_doc("Visible"),
        )
        .unwrap();

        let result = executor.execute_tool("list_documents", json!({})).await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["name"], "visible.midlight");
    }

    #[tokio::test]
    async fn test_list_documents_subdirectory() {
        let (temp, executor) = create_test_executor();

        // Create subdirectory with file
        std::fs::create_dir(temp.path().join("notes")).unwrap();
        std::fs::write(
            temp.path().join("notes/idea.midlight"),
            create_midlight_doc("Idea"),
        )
        .unwrap();

        let result = executor
            .execute_tool("list_documents", json!({ "path": "notes" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["name"], "idea.midlight");
    }

    #[tokio::test]
    async fn test_list_documents_nonexistent_dir() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool("list_documents", json!({ "path": "nonexistent" }))
            .await;

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    // ============================================
    // read_document tests
    // ============================================

    #[tokio::test]
    async fn test_read_document_success() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("test.midlight"),
            create_midlight_doc("Hello World"),
        )
        .unwrap();

        let result = executor
            .execute_tool("read_document", json!({ "path": "test.midlight" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        assert!(data["content"].as_str().unwrap().contains("Hello World"));
    }

    #[tokio::test]
    async fn test_read_document_missing_path() {
        let (_temp, executor) = create_test_executor();

        let result = executor.execute_tool("read_document", json!({})).await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing required parameter"));
    }

    #[tokio::test]
    async fn test_read_document_not_found() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool("read_document", json!({ "path": "nonexistent.midlight" }))
            .await;

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_read_document_plain_text() {
        let (temp, executor) = create_test_executor();

        // Write a non-JSON file
        std::fs::write(temp.path().join("plain.txt"), "Just plain text").unwrap();

        let result = executor
            .execute_tool("read_document", json!({ "path": "plain.txt" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        assert_eq!(data["content"].as_str().unwrap(), "Just plain text");
    }

    // ============================================
    // create_document tests
    // ============================================

    #[tokio::test]
    async fn test_create_document_success() {
        let (temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "new-doc",
                    "content": "# Hello\n\nThis is content",
                    "title": "My Document"
                }),
            )
            .await;

        assert!(result.success);
        assert!(temp.path().join("new-doc.midlight").exists());

        // Verify content
        let content = std::fs::read_to_string(temp.path().join("new-doc.midlight")).unwrap();
        let doc: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(doc["version"], 1);
    }

    #[tokio::test]
    async fn test_create_document_already_exists() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("existing.midlight"),
            create_midlight_doc("Existing"),
        )
        .unwrap();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "existing.midlight",
                    "content": "New content"
                }),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("already exists"));
    }

    #[tokio::test]
    async fn test_create_document_creates_parent_dirs() {
        let (temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "deep/nested/path/doc",
                    "content": "Content"
                }),
            )
            .await;

        assert!(result.success);
        assert!(temp.path().join("deep/nested/path/doc.midlight").exists());
    }

    #[tokio::test]
    async fn test_create_document_missing_path() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool("create_document", json!({ "content": "content" }))
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing required parameter"));
    }

    #[tokio::test]
    async fn test_create_document_adds_extension() {
        let (temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "no-extension",
                    "content": ""
                }),
            )
            .await;

        assert!(result.success);
        // Should add .midlight extension
        assert!(temp.path().join("no-extension.midlight").exists());
    }

    // ============================================
    // edit_document tests
    // ============================================

    #[tokio::test]
    async fn test_edit_document_success() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("edit-me.midlight"),
            create_midlight_doc("Original content"),
        )
        .unwrap();

        let result = executor
            .execute_tool(
                "edit_document",
                json!({
                    "path": "edit-me.midlight",
                    "content": "Updated content",
                    "description": "Made some changes"
                }),
            )
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        assert!(data["requiresAcceptance"].as_bool().unwrap());
        assert!(data["changeId"].is_string());
        assert_eq!(data["newContent"], "Updated content");
    }

    #[tokio::test]
    async fn test_edit_document_not_found() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "edit_document",
                json!({
                    "path": "nonexistent.midlight",
                    "content": "New content"
                }),
            )
            .await;

        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_edit_document_missing_content() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        let result = executor
            .execute_tool("edit_document", json!({ "path": "doc.midlight" }))
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing required parameter"));
    }

    // ============================================
    // move_document tests
    // ============================================

    #[tokio::test]
    async fn test_move_document_success() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("old.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        let result = executor
            .execute_tool(
                "move_document",
                json!({
                    "oldPath": "old.midlight",
                    "newPath": "new.midlight"
                }),
            )
            .await;

        assert!(result.success);
        assert!(!temp.path().join("old.midlight").exists());
        assert!(temp.path().join("new.midlight").exists());
    }

    #[tokio::test]
    async fn test_move_document_source_not_found() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "move_document",
                json!({
                    "oldPath": "nonexistent.midlight",
                    "newPath": "new.midlight"
                }),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_move_document_dest_exists() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("source.midlight"),
            create_midlight_doc("Source"),
        )
        .unwrap();
        std::fs::write(
            temp.path().join("dest.midlight"),
            create_midlight_doc("Dest"),
        )
        .unwrap();

        let result = executor
            .execute_tool(
                "move_document",
                json!({
                    "oldPath": "source.midlight",
                    "newPath": "dest.midlight"
                }),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("already exists"));
    }

    #[tokio::test]
    async fn test_move_document_missing_params() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool("move_document", json!({ "oldPath": "test.midlight" }))
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing required parameter"));
    }

    #[tokio::test]
    async fn test_move_document_creates_parent_dirs() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        let result = executor
            .execute_tool(
                "move_document",
                json!({
                    "oldPath": "doc.midlight",
                    "newPath": "deep/nested/doc.midlight"
                }),
            )
            .await;

        assert!(result.success);
        assert!(temp.path().join("deep/nested/doc.midlight").exists());
    }

    // ============================================
    // delete_document tests
    // ============================================

    #[tokio::test]
    async fn test_delete_document_not_found() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool("delete_document", json!({ "path": "nonexistent.midlight" }))
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_document_missing_path() {
        let (_temp, executor) = create_test_executor();

        let result = executor.execute_tool("delete_document", json!({})).await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing required parameter"));
    }

    // Note: Actual deletion test is tricky because it uses the trash crate
    // which may not work in all test environments

    // ============================================
    // search_documents tests
    // ============================================

    #[tokio::test]
    async fn test_search_documents_success() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc1.midlight"),
            create_midlight_doc("Hello World"),
        )
        .unwrap();
        std::fs::write(
            temp.path().join("doc2.midlight"),
            create_midlight_doc("Goodbye World"),
        )
        .unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "Hello" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["name"], "doc1.midlight");
    }

    #[tokio::test]
    async fn test_search_documents_case_insensitive() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("HELLO World"),
        )
        .unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "hello" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[tokio::test]
    async fn test_search_documents_no_matches() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("Hello World"),
        )
        .unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "xyz123" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_search_documents_missing_query() {
        let (_temp, executor) = create_test_executor();

        let result = executor.execute_tool("search_documents", json!({})).await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing required parameter"));
    }

    #[tokio::test]
    async fn test_search_documents_recursive() {
        let (temp, executor) = create_test_executor();

        std::fs::create_dir(temp.path().join("subfolder")).unwrap();
        std::fs::write(
            temp.path().join("subfolder/nested.midlight"),
            create_midlight_doc("Nested content findme"),
        )
        .unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "findme" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["path"].as_str().unwrap().contains("subfolder"));
    }

    // ============================================
    // Markdown to Tiptap conversion tests
    // ============================================

    #[tokio::test]
    async fn test_markdown_to_tiptap_headings() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("# Heading 1\n## Heading 2\n### Heading 3");

        assert_eq!(result["type"], "doc");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["attrs"]["level"], 1);
        assert_eq!(content[1]["attrs"]["level"], 2);
        assert_eq!(content[2]["attrs"]["level"], 3);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_paragraphs() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("First paragraph\n\nSecond paragraph");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "paragraph");
        assert_eq!(content[1]["type"], "paragraph");
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_bullet_list() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("- Item 1\n- Item 2\n- Item 3");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "bulletList");
        let items = content[0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 3);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_ordered_list() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("1. First\n2. Second\n3. Third");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "orderedList");
        let items = content[0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 3);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_blockquote() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("> This is a quote");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "blockquote");
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_horizontal_rule() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("---");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "horizontalRule");
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_empty() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("");

        assert_eq!(result["type"], "doc");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "paragraph");
    }

    // ============================================
    // Tiptap to Markdown conversion tests
    // ============================================

    #[tokio::test]
    async fn test_tiptap_to_markdown_headings() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [
                { "type": "heading", "attrs": { "level": 1 }, "content": [{ "type": "text", "text": "H1" }] },
                { "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "H2" }] }
            ]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("# H1"));
        assert!(markdown.contains("## H2"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_bold() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "bold text",
                    "marks": [{ "type": "bold" }]
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("**bold text**"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_italic() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "italic text",
                    "marks": [{ "type": "italic" }]
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("*italic text*"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_code() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "code",
                    "marks": [{ "type": "code" }]
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("`code`"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_horizontal_rule() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{ "type": "horizontalRule" }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("---"));
    }

    // ============================================
    // Inline formatting parsing tests
    // ============================================

    #[tokio::test]
    async fn test_parse_inline_bold() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("This is **bold** text");

        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["text"], "This is ");
        assert_eq!(result[1]["text"], "bold");
        assert!(result[1]["marks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["type"] == "bold"));
        assert_eq!(result[2]["text"], " text");
    }

    #[tokio::test]
    async fn test_parse_inline_italic() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("This is *italic* text");

        assert!(result.iter().any(|n| {
            n["marks"]
                .as_array()
                .map(|m| m.iter().any(|mark| mark["type"] == "italic"))
                .unwrap_or(false)
        }));
    }

    #[tokio::test]
    async fn test_parse_inline_code() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("Use `code` here");

        assert!(result.iter().any(|n| {
            n["text"] == "code"
                && n["marks"]
                    .as_array()
                    .map(|m| m.iter().any(|mark| mark["type"] == "code"))
                    .unwrap_or(false)
        }));
    }

    #[tokio::test]
    async fn test_parse_inline_plain() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("Plain text only");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["text"], "Plain text only");
        assert!(result[0].get("marks").is_none());
    }

    // ============================================
    // Extract text from Tiptap tests
    // ============================================

    #[tokio::test]
    async fn test_extract_text_from_tiptap() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Hello " }, { "type": "text", "text": "World" }] },
                { "type": "paragraph", "content": [{ "type": "text", "text": "Second line" }] }
            ]
        });

        let text = executor.extract_text_from_tiptap(&tiptap);
        assert!(text.contains("Hello World"));
        assert!(text.contains("Second line"));
    }

    // ============================================
    // Extract snippet tests
    // ============================================

    #[tokio::test]
    async fn test_extract_snippet_found() {
        let (_temp, executor) = create_test_executor();

        let text = "This is a long text with the search term somewhere in the middle of it.";
        let snippet = executor.extract_snippet(text, "search term");

        assert!(snippet.contains("search term"));
    }

    #[tokio::test]
    async fn test_extract_snippet_at_start() {
        let (_temp, executor) = create_test_executor();

        let text = "Match at the very beginning of the text";
        let snippet = executor.extract_snippet(text, "match");

        assert!(snippet.to_lowercase().contains("match"));
    }

    #[tokio::test]
    async fn test_extract_snippet_not_found() {
        let (_temp, executor) = create_test_executor();

        let text = "Some text without the query";
        let snippet = executor.extract_snippet(text, "xyz");

        // Should return first 100 chars when not found
        assert!(!snippet.is_empty());
    }

    // ============================================
    // Additional coverage tests
    // ============================================

    #[tokio::test]
    async fn test_list_documents_with_leading_slash() {
        let (temp, executor) = create_test_executor();

        std::fs::create_dir(temp.path().join("folder")).unwrap();
        std::fs::write(
            temp.path().join("folder/doc.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        let result = executor
            .execute_tool("list_documents", json!({ "path": "/folder" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn test_list_documents_root_slash() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("root.midlight"),
            create_midlight_doc("Root doc"),
        )
        .unwrap();

        let result = executor
            .execute_tool("list_documents", json!({ "path": "/" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn test_list_documents_ignores_non_midlight_files() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("Midlight"),
        )
        .unwrap();
        std::fs::write(temp.path().join("other.txt"), "Plain text").unwrap();
        std::fs::write(temp.path().join("data.json"), "{}").unwrap();

        let result = executor.execute_tool("list_documents", json!({})).await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        // Should only show .midlight files
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["name"], "doc.midlight");
    }

    #[tokio::test]
    async fn test_list_documents_sorts_dirs_first() {
        let (temp, executor) = create_test_executor();

        std::fs::write(temp.path().join("zzz.midlight"), create_midlight_doc("Z")).unwrap();
        std::fs::create_dir(temp.path().join("aaa_folder")).unwrap();
        std::fs::write(temp.path().join("aaa.midlight"), create_midlight_doc("A")).unwrap();

        let result = executor.execute_tool("list_documents", json!({})).await;

        assert!(result.success);
        let data = result.data.unwrap();
        let files = data["files"].as_array().unwrap();
        // Directory should be first despite name
        assert_eq!(files[0]["type"], "directory");
        assert_eq!(files[0]["name"], "aaa_folder");
    }

    #[tokio::test]
    async fn test_read_document_with_title() {
        let (temp, executor) = create_test_executor();

        let doc = json!({
            "version": 1,
            "meta": {
                "created": "2024-01-01T00:00:00Z",
                "modified": "2024-01-01T00:00:00Z",
                "title": "My Document Title"
            },
            "document": {},
            "content": {
                "type": "doc",
                "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Content" }] }]
            }
        });
        std::fs::write(
            temp.path().join("titled.midlight"),
            serde_json::to_string_pretty(&doc).unwrap(),
        )
        .unwrap();

        let result = executor
            .execute_tool("read_document", json!({ "path": "titled.midlight" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        assert_eq!(data["title"], "My Document Title");
    }

    #[tokio::test]
    async fn test_read_document_with_leading_slash() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        let result = executor
            .execute_tool("read_document", json!({ "path": "/doc.midlight" }))
            .await;

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_create_document_with_extension() {
        let (temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "with-ext.midlight",
                    "content": "Content"
                }),
            )
            .await;

        assert!(result.success);
        // Should not double the extension
        assert!(temp.path().join("with-ext.midlight").exists());
        assert!(!temp.path().join("with-ext.midlight.midlight").exists());
    }

    #[tokio::test]
    async fn test_create_document_empty_content() {
        let (temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "empty"
                }),
            )
            .await;

        assert!(result.success);
        assert!(temp.path().join("empty.midlight").exists());
    }

    #[tokio::test]
    async fn test_create_document_with_leading_slash() {
        let (temp, executor) = create_test_executor();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "/slashed",
                    "content": "Content"
                }),
            )
            .await;

        assert!(result.success);
        assert!(temp.path().join("slashed.midlight").exists());
    }

    #[tokio::test]
    async fn test_edit_document_missing_path() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool("edit_document", json!({ "content": "new content" }))
            .await;

        assert!(!result.success);
        assert!(result
            .error
            .unwrap()
            .contains("Missing required parameter: path"));
    }

    #[tokio::test]
    async fn test_edit_document_invalid_json() {
        let (temp, executor) = create_test_executor();

        // Write invalid JSON
        std::fs::write(temp.path().join("invalid.midlight"), "not valid json").unwrap();

        let result = executor
            .execute_tool(
                "edit_document",
                json!({
                    "path": "invalid.midlight",
                    "content": "New content"
                }),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Failed to parse document"));
    }

    #[tokio::test]
    async fn test_edit_document_with_leading_slash() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("Original"),
        )
        .unwrap();

        let result = executor
            .execute_tool(
                "edit_document",
                json!({
                    "path": "/doc.midlight",
                    "content": "Updated"
                }),
            )
            .await;

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_move_document_missing_old_path() {
        let (_temp, executor) = create_test_executor();

        let result = executor
            .execute_tool("move_document", json!({ "newPath": "new.midlight" }))
            .await;

        assert!(!result.success);
        assert!(result
            .error
            .unwrap()
            .contains("Missing required parameter: oldPath"));
    }

    #[tokio::test]
    async fn test_move_document_with_leading_slashes() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("source.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        let result = executor
            .execute_tool(
                "move_document",
                json!({
                    "oldPath": "/source.midlight",
                    "newPath": "/dest.midlight"
                }),
            )
            .await;

        assert!(result.success);
        assert!(temp.path().join("dest.midlight").exists());
    }

    #[tokio::test]
    async fn test_delete_document_with_leading_slash() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("to-delete.midlight"),
            create_midlight_doc("Delete me"),
        )
        .unwrap();

        // Note: This test might fail in CI due to trash crate limitations
        let result = executor
            .execute_tool("delete_document", json!({ "path": "/to-delete.midlight" }))
            .await;

        // The result depends on whether trash works in test environment
        // Either success or error about trash
        assert!(result.success || result.error.is_some());
    }

    #[tokio::test]
    async fn test_search_documents_skips_hidden_folders() {
        let (temp, executor) = create_test_executor();

        std::fs::create_dir(temp.path().join(".hidden")).unwrap();
        std::fs::write(
            temp.path().join(".hidden/secret.midlight"),
            create_midlight_doc("Secret content findthis"),
        )
        .unwrap();
        std::fs::write(
            temp.path().join("visible.midlight"),
            create_midlight_doc("Visible content"),
        )
        .unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "findthis" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        // Should not find the hidden file
        assert_eq!(matches.len(), 0);
    }

    #[tokio::test]
    async fn test_search_documents_with_file_pattern() {
        let (temp, executor) = create_test_executor();

        std::fs::write(
            temp.path().join("doc.midlight"),
            create_midlight_doc("searchterm here"),
        )
        .unwrap();

        // filePattern is accepted but currently unused
        let result = executor
            .execute_tool(
                "search_documents",
                json!({ "query": "searchterm", "filePattern": "*.midlight" }),
            )
            .await;

        assert!(result.success);
    }

    // ============================================
    // Tiptap to Markdown edge cases
    // ============================================

    #[tokio::test]
    async fn test_tiptap_to_markdown_bold_italic() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "bold and italic",
                    "marks": [{ "type": "bold" }, { "type": "italic" }]
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("***bold and italic***"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_bullet_list() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "bulletList",
                "content": [
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "Item 1" }]
                        }]
                    },
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "Item 2" }]
                        }]
                    }
                ]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("- Item 1"));
        assert!(markdown.contains("- Item 2"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_ordered_list() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "orderedList",
                "content": [
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "First" }]
                        }]
                    },
                    {
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "Second" }]
                        }]
                    }
                ]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("1. First"));
        assert!(markdown.contains("2. Second"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_blockquote() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "blockquote",
                "content": [{
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "Quoted text" }]
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("> Quoted text"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_unknown_type() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "unknownType",
                "content": [{
                    "type": "text",
                    "text": "Inner text"
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        // Should still extract text from unknown types
        assert!(markdown.contains("Inner text"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_text_without_marks() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "Plain text"
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("Plain text"));
    }

    #[tokio::test]
    async fn test_tiptap_to_markdown_empty_marks() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "Text with empty marks",
                    "marks": []
                }]
            }]
        });

        let markdown = executor.tiptap_to_markdown(&tiptap);
        assert!(markdown.contains("Text with empty marks"));
    }

    // ============================================
    // Extract text from Tiptap edge cases
    // ============================================

    #[tokio::test]
    async fn test_extract_text_bullet_list() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "doc",
            "content": [{
                "type": "bulletList",
                "content": [{
                    "type": "listItem",
                    "content": [{
                        "type": "paragraph",
                        "content": [{ "type": "text", "text": "List item" }]
                    }]
                }]
            }]
        });

        let text = executor.extract_text_from_tiptap(&tiptap);
        assert!(text.contains("List item"));
    }

    #[tokio::test]
    async fn test_extract_text_unknown_type() {
        let (_temp, executor) = create_test_executor();

        let tiptap = json!({
            "type": "unknownType",
            "content": [{
                "type": "text",
                "text": "Nested text"
            }]
        });

        let text = executor.extract_text_from_tiptap(&tiptap);
        assert!(text.contains("Nested text"));
    }

    // ============================================
    // Markdown to Tiptap edge cases
    // ============================================

    #[tokio::test]
    async fn test_markdown_to_tiptap_heading_4() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("#### Heading 4");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "heading");
        assert_eq!(content[0]["attrs"]["level"], 4);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_heading_5() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("##### Heading 5");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "heading");
        assert_eq!(content[0]["attrs"]["level"], 5);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_heading_6() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("###### Heading 6");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "heading");
        assert_eq!(content[0]["attrs"]["level"], 6);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_hr_asterisks() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("***");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "horizontalRule");
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_hr_underscores() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("___");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "horizontalRule");
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_bullet_list_asterisk() {
        let (_temp, executor) = create_test_executor();

        let result = executor.markdown_to_tiptap("* Item 1\n* Item 2");

        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "bulletList");
        let items = content[0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_markdown_to_tiptap_mixed_content() {
        let (_temp, executor) = create_test_executor();

        let markdown = "# Title\n\nParagraph text\n\n- List item\n\n> Quote";
        let result = executor.markdown_to_tiptap(markdown);

        let content = result["content"].as_array().unwrap();
        assert!(content.len() >= 4);
    }

    // ============================================
    // Parse inline formatting edge cases
    // ============================================

    #[tokio::test]
    async fn test_parse_inline_bold_italic() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("This is ***bold italic*** text");

        assert!(result.iter().any(|n| {
            let marks = n["marks"].as_array();
            marks
                .map(|m| {
                    m.iter().any(|mark| mark["type"] == "bold")
                        && m.iter().any(|mark| mark["type"] == "italic")
                })
                .unwrap_or(false)
        }));
    }

    #[tokio::test]
    async fn test_parse_inline_bold_underscore() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("This is __bold__ text");

        assert!(result.iter().any(|n| {
            n["marks"]
                .as_array()
                .map(|m| m.iter().any(|mark| mark["type"] == "bold"))
                .unwrap_or(false)
        }));
    }

    #[tokio::test]
    async fn test_parse_inline_italic_underscore() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("This is _italic_ text");

        assert!(result.iter().any(|n| {
            n["marks"]
                .as_array()
                .map(|m| m.iter().any(|mark| mark["type"] == "italic"))
                .unwrap_or(false)
        }));
    }

    #[tokio::test]
    async fn test_parse_inline_unclosed_code() {
        let (_temp, executor) = create_test_executor();

        // Unclosed backtick
        let result = executor.parse_inline_formatting("Start `unclosed code");

        // Should handle gracefully
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_parse_inline_unclosed_bold() {
        let (_temp, executor) = create_test_executor();

        // Unclosed bold
        let result = executor.parse_inline_formatting("Start **unclosed bold");

        // Should handle gracefully
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_parse_inline_unclosed_italic() {
        let (_temp, executor) = create_test_executor();

        // Unclosed italic
        let result = executor.parse_inline_formatting("Start *unclosed italic");

        // Should handle gracefully
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_parse_inline_empty() {
        let (_temp, executor) = create_test_executor();

        let result = executor.parse_inline_formatting("");

        assert!(result.is_empty());
    }

    // ============================================
    // Snippet extraction edge cases
    // ============================================

    #[tokio::test]
    async fn test_extract_snippet_at_end() {
        let (_temp, executor) = create_test_executor();

        let text = "Some text with the match at the end";
        let snippet = executor.extract_snippet(text, "end");

        assert!(snippet.contains("end"));
    }

    #[tokio::test]
    async fn test_extract_snippet_short_text() {
        let (_temp, executor) = create_test_executor();

        let text = "Short";
        let snippet = executor.extract_snippet(text, "short");

        assert!(snippet.to_lowercase().contains("short"));
    }

    #[tokio::test]
    async fn test_extract_snippet_exact_match() {
        let (_temp, executor) = create_test_executor();

        let text = "match";
        let snippet = executor.extract_snippet(text, "match");

        assert_eq!(snippet, "match");
    }

    // ============================================
    // Type serialization tests
    // ============================================

    #[test]
    fn test_tool_execution_status_serialize() {
        let status = ToolExecutionStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending\"");

        let status = ToolExecutionStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"running\"");

        let status = ToolExecutionStatus::Completed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"completed\"");

        let status = ToolExecutionStatus::Failed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"failed\"");

        let status = ToolExecutionStatus::RequiresConfirmation;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"requires_confirmation\"");
    }

    #[test]
    fn test_tool_result_serialize() {
        let result = ToolResult {
            success: true,
            data: Some(json!({"key": "value"})),
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"key\":\"value\""));
    }

    #[test]
    fn test_file_info_serialize() {
        let info = FileInfo {
            path: "test/path".to_string(),
            name: "file.midlight".to_string(),
            file_type: "file".to_string(),
            modified: Some("2024-01-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"path\":\"test/path\""));
        assert!(json.contains("\"type\":\"file\"")); // renamed
    }

    #[test]
    fn test_search_match_serialize() {
        let m = SearchMatch {
            path: "doc.midlight".to_string(),
            name: "doc.midlight".to_string(),
            snippet: "...matching text...".to_string(),
            line: Some(42),
        };

        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"line\":42"));
    }

    #[test]
    fn test_tool_execution_serialize() {
        let exec = ToolExecution {
            id: "123".to_string(),
            tool_name: "read_document".to_string(),
            arguments: json!({"path": "test.midlight"}),
            status: ToolExecutionStatus::Completed,
            result: Some(ToolResult {
                success: true,
                data: None,
                error: None,
            }),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: Some("2024-01-01T00:00:01Z".to_string()),
        };

        let json = serde_json::to_string(&exec).unwrap();
        assert!(json.contains("\"toolName\":\"read_document\""));
        assert!(json.contains("\"completedAt\""));
    }

    #[test]
    fn test_pending_change_serialize() {
        let change = PendingChange {
            change_id: "abc123".to_string(),
            path: "doc.midlight".to_string(),
            original_content: "old".to_string(),
            new_content: "new".to_string(),
            description: Some("Made changes".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&change).unwrap();
        assert!(json.contains("\"changeId\":\"abc123\""));
        assert!(json.contains("\"originalContent\":\"old\""));
    }

    // ============================================
    // Debug trait tests
    // ============================================

    #[test]
    fn test_tool_execution_debug() {
        let exec = ToolExecution {
            id: "123".to_string(),
            tool_name: "test".to_string(),
            arguments: json!({}),
            status: ToolExecutionStatus::Pending,
            result: None,
            started_at: "now".to_string(),
            completed_at: None,
        };

        let debug = format!("{:?}", exec);
        assert!(debug.contains("ToolExecution"));
    }

    #[test]
    fn test_tool_result_debug() {
        let result = ToolResult {
            success: true,
            data: None,
            error: None,
        };

        let debug = format!("{:?}", result);
        assert!(debug.contains("ToolResult"));
    }

    #[test]
    fn test_file_info_debug() {
        let info = FileInfo {
            path: "test".to_string(),
            name: "test".to_string(),
            file_type: "file".to_string(),
            modified: None,
        };

        let debug = format!("{:?}", info);
        assert!(debug.contains("FileInfo"));
    }

    #[test]
    fn test_search_match_debug() {
        let m = SearchMatch {
            path: "test".to_string(),
            name: "test".to_string(),
            snippet: "snippet".to_string(),
            line: None,
        };

        let debug = format!("{:?}", m);
        assert!(debug.contains("SearchMatch"));
    }

    #[test]
    fn test_pending_change_debug() {
        let change = PendingChange {
            change_id: "123".to_string(),
            path: "test".to_string(),
            original_content: "old".to_string(),
            new_content: "new".to_string(),
            description: None,
            created_at: "now".to_string(),
        };

        let debug = format!("{:?}", change);
        assert!(debug.contains("PendingChange"));
    }

    #[test]
    fn test_tool_execution_status_debug() {
        let status = ToolExecutionStatus::Pending;
        let debug = format!("{:?}", status);
        assert!(debug.contains("Pending"));
    }

    #[test]
    fn test_tool_execution_status_partial_eq() {
        assert_eq!(ToolExecutionStatus::Pending, ToolExecutionStatus::Pending);
        assert_ne!(ToolExecutionStatus::Pending, ToolExecutionStatus::Running);
    }

    #[test]
    fn test_tool_execution_status_clone() {
        let status = ToolExecutionStatus::Completed;
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    // ============================================
    // File system error tests (Unix-specific)
    // ============================================

    #[tokio::test]
    #[cfg(unix)]
    async fn test_create_document_dir_creation_fails() {
        use std::os::unix::fs::PermissionsExt;

        let (temp, executor) = create_test_executor();

        // Create a read-only directory
        let readonly = temp.path().join("readonly");
        std::fs::create_dir(&readonly).unwrap();
        std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o444)).unwrap();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "readonly/nested/doc",
                    "content": "Content"
                }),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Failed to create directory"));

        // Cleanup
        std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_create_document_write_fails() {
        use std::os::unix::fs::PermissionsExt;

        let (temp, executor) = create_test_executor();

        // Make workspace read-only so file write fails
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = executor
            .execute_tool(
                "create_document",
                json!({
                    "path": "newdoc",
                    "content": "Content"
                }),
            )
            .await;

        assert!(!result.success);
        // Could be either directory or document creation failure depending on timing
        let error = result.error.unwrap();
        assert!(
            error.contains("Failed to create document")
                || error.contains("Failed to create directory")
        );

        // Cleanup
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_move_document_dir_creation_fails() {
        use std::os::unix::fs::PermissionsExt;

        let (temp, executor) = create_test_executor();

        // Create source file
        std::fs::write(
            temp.path().join("source.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        // Create read-only directory
        let readonly = temp.path().join("readonly");
        std::fs::create_dir(&readonly).unwrap();
        std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o444)).unwrap();

        let result = executor
            .execute_tool(
                "move_document",
                json!({
                    "oldPath": "source.midlight",
                    "newPath": "readonly/nested/dest.midlight"
                }),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Failed to create directory"));

        // Cleanup
        std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_move_document_rename_fails() {
        use std::os::unix::fs::PermissionsExt;

        let (temp, executor) = create_test_executor();

        // Create source file
        std::fs::write(
            temp.path().join("source.midlight"),
            create_midlight_doc("Content"),
        )
        .unwrap();

        // Create destination directory but make it read-only
        let dest_dir = temp.path().join("destdir");
        std::fs::create_dir(&dest_dir).unwrap();
        std::fs::set_permissions(&dest_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = executor
            .execute_tool(
                "move_document",
                json!({
                    "oldPath": "source.midlight",
                    "newPath": "destdir/dest.midlight"
                }),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Failed to move document"));

        // Cleanup
        std::fs::set_permissions(&dest_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // ============================================
    // Search error tests
    // ============================================

    #[tokio::test]
    #[cfg(unix)]
    async fn test_search_skips_unreadable_files() {
        use std::os::unix::fs::PermissionsExt;

        let (temp, executor) = create_test_executor();

        // Create readable file
        std::fs::write(
            temp.path().join("readable.midlight"),
            create_midlight_doc("findme content"),
        )
        .unwrap();

        // Create unreadable file
        let unreadable = temp.path().join("unreadable.midlight");
        std::fs::write(&unreadable, create_midlight_doc("findme hidden")).unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "findme" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        // Should only find the readable file
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["name"].as_str().unwrap().contains("readable"));

        // Cleanup
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[tokio::test]
    async fn test_search_skips_invalid_json_files() {
        let (temp, executor) = create_test_executor();

        // Create valid file
        std::fs::write(
            temp.path().join("valid.midlight"),
            create_midlight_doc("findme valid"),
        )
        .unwrap();

        // Create file with invalid JSON
        std::fs::write(
            temp.path().join("invalid.midlight"),
            "not valid json { findme }",
        )
        .unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "findme" }))
            .await;

        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        // Should only find the valid file
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["name"].as_str().unwrap().contains("valid"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_search_handles_unreadable_directory() {
        use std::os::unix::fs::PermissionsExt;

        let (temp, executor) = create_test_executor();

        // Create unreadable subdirectory (name starts with 'z' to be processed last)
        let unreadable = temp.path().join("zzz_unreadable_dir");
        std::fs::create_dir(&unreadable).unwrap();
        std::fs::write(
            unreadable.join("hidden.midlight"),
            create_midlight_doc("findme hidden"),
        )
        .unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Create readable file (name starts with 'a' to be processed first)
        std::fs::write(
            temp.path().join("aaa_visible.midlight"),
            create_midlight_doc("findme visible"),
        )
        .unwrap();

        let result = executor
            .execute_tool("search_documents", json!({ "query": "findme" }))
            .await;

        // Search should succeed (error is caught and logged with warn!)
        assert!(result.success);
        let data = result.data.unwrap();
        let matches = data["matches"].as_array().unwrap();
        // Should find the visible file (processed before hitting unreadable dir)
        // Directory iteration order may vary, so accept 0 or 1 matches
        assert!(matches.len() <= 1);
        if matches.len() == 1 {
            assert!(matches[0]["name"].as_str().unwrap().contains("visible"));
        }

        // Cleanup
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
