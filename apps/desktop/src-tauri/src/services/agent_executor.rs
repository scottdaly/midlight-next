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
                            } else {
                                if has_bold && has_italic {
                                    formatted = format!("***{}***", formatted);
                                } else if has_bold {
                                    formatted = format!("**{}**", formatted);
                                } else if has_italic {
                                    formatted = format!("*{}*", formatted);
                                }
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
