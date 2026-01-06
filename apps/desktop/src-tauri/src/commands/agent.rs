// Agent Commands - Tauri IPC handlers for AI agent tool execution

use crate::services::agent_executor::{AgentExecutor, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::debug;

// ============================================================================
// Command Input Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteToolRequest {
    pub workspace_root: String,
    pub tool_name: String,
    pub arguments: Value,
}

// ============================================================================
// Commands
// ============================================================================

/// Execute a single tool
#[tauri::command]
pub async fn agent_execute_tool(request: ExecuteToolRequest) -> Result<ToolResult, String> {
    debug!(
        "agent_execute_tool: {} in {}",
        request.tool_name, request.workspace_root
    );

    let executor = AgentExecutor::new(PathBuf::from(&request.workspace_root));
    let result = executor
        .execute_tool(&request.tool_name, request.arguments)
        .await;

    Ok(result)
}

/// List available tools
#[tauri::command]
pub fn agent_list_tools() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "list_documents".to_string(),
            description: "List all documents and folders in a directory".to_string(),
            is_destructive: false,
        },
        ToolInfo {
            name: "read_document".to_string(),
            description: "Read the full content of a document".to_string(),
            is_destructive: false,
        },
        ToolInfo {
            name: "create_document".to_string(),
            description: "Create a new document with the specified content".to_string(),
            is_destructive: false,
        },
        ToolInfo {
            name: "edit_document".to_string(),
            description: "Edit an existing document".to_string(),
            is_destructive: false,
        },
        ToolInfo {
            name: "move_document".to_string(),
            description: "Move or rename a document".to_string(),
            is_destructive: false,
        },
        ToolInfo {
            name: "delete_document".to_string(),
            description: "Delete a document (moves to trash)".to_string(),
            is_destructive: true,
        },
        ToolInfo {
            name: "search_documents".to_string(),
            description: "Search for documents containing specific text".to_string(),
            is_destructive: false,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub is_destructive: bool,
}
