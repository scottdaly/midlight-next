//! Shared test utilities and helpers.
//!
//! This module provides common utilities for testing throughout the codebase.

#![allow(dead_code)] // These utilities will be used by tests in other modules

use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary workspace directory with the standard `.midlight` structure.
///
/// Returns the TempDir (which cleans up on drop) and the path to the workspace.
pub fn create_test_workspace() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("Failed to create temp directory");
    let workspace_path = temp.path().to_path_buf();

    // Create the standard .midlight directory structure
    let midlight_dir = workspace_path.join(".midlight");
    std::fs::create_dir_all(&midlight_dir).expect("Failed to create .midlight directory");
    std::fs::create_dir_all(midlight_dir.join("objects"))
        .expect("Failed to create objects directory");
    std::fs::create_dir_all(midlight_dir.join("checkpoints"))
        .expect("Failed to create checkpoints directory");
    std::fs::create_dir_all(midlight_dir.join("images"))
        .expect("Failed to create images directory");
    std::fs::create_dir_all(midlight_dir.join("recovery"))
        .expect("Failed to create recovery directory");

    (temp, workspace_path)
}

/// Create a temporary workspace with a test document.
pub fn create_test_workspace_with_document(
    filename: &str,
    content: &str,
) -> (TempDir, PathBuf, PathBuf) {
    let (temp, workspace_path) = create_test_workspace();
    let doc_path = workspace_path.join(filename);

    std::fs::write(&doc_path, content).expect("Failed to write test document");

    (temp, workspace_path, doc_path)
}

/// Create a simple test markdown document.
pub fn sample_markdown() -> &'static str {
    r#"# Test Document

This is a test document with some content.

## Section 1

Some text in section 1.

## Section 2

Some text in section 2.

- List item 1
- List item 2
- List item 3
"#
}

/// Create a sample Tiptap JSON document.
pub fn sample_tiptap_json() -> serde_json::Value {
    serde_json::json!({
        "type": "doc",
        "content": [
            {
                "type": "heading",
                "attrs": { "level": 1 },
                "content": [
                    { "type": "text", "text": "Test Document" }
                ]
            },
            {
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": "This is a test paragraph." }
                ]
            }
        ]
    })
}

/// Create sample checkpoint data.
pub fn sample_checkpoint_json(checkpoint_id: &str, content_hash: &str) -> serde_json::Value {
    serde_json::json!({
        "id": checkpoint_id,
        "timestamp": "2024-01-01T00:00:00Z",
        "content_hash": content_hash,
        "sidecar_hash": null,
        "trigger": "manual",
        "label": null,
        "description": null,
        "change_size": 100
    })
}

/// Sample user data for auth tests.
pub fn sample_user_json() -> serde_json::Value {
    serde_json::json!({
        "id": "user_123",
        "email": "test@example.com",
        "display_name": "Test User",
        "avatar_url": null,
        "created_at": "2024-01-01T00:00:00Z"
    })
}

/// Sample subscription data.
pub fn sample_subscription_json() -> serde_json::Value {
    serde_json::json!({
        "tier": "pro",
        "status": "active",
        "current_period_end": "2025-01-01T00:00:00Z",
        "cancel_at_period_end": false
    })
}

/// Sample quota data.
pub fn sample_quota_json() -> serde_json::Value {
    serde_json::json!({
        "used": 1000,
        "limit": 10000,
        "reset_at": "2024-02-01T00:00:00Z"
    })
}

/// Sample LLM chat response.
pub fn sample_chat_response_json() -> serde_json::Value {
    serde_json::json!({
        "id": "msg_123",
        "content": "This is a test response from the LLM.",
        "model": "gpt-4",
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "total_tokens": 30
        }
    })
}

/// Sample import analysis result.
pub fn sample_import_analysis_json() -> serde_json::Value {
    serde_json::json!({
        "source_type": "obsidian",
        "total_files": 100,
        "markdown_files": 80,
        "attachments": 20,
        "total_size_bytes": 1048576,
        "features": {
            "has_wiki_links": true,
            "has_callouts": true,
            "has_dataview": false,
            "has_front_matter": true
        }
    })
}

/// Assert that two JSON values are equal, with better error messages.
#[macro_export]
macro_rules! assert_json_eq {
    ($left:expr, $right:expr) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if *left_val != *right_val {
                    panic!(
                        "JSON values not equal:\n\nLeft:\n{}\n\nRight:\n{}\n",
                        serde_json::to_string_pretty(left_val).unwrap(),
                        serde_json::to_string_pretty(right_val).unwrap()
                    );
                }
            }
        }
    };
}

/// Assert that a result is Ok and return the value.
#[macro_export]
macro_rules! assert_ok {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(e) => panic!("Expected Ok, got Err: {:?}", e),
        }
    };
}

/// Assert that a result is Err and return the error.
#[macro_export]
macro_rules! assert_err {
    ($result:expr) => {
        match $result {
            Ok(v) => panic!("Expected Err, got Ok: {:?}", v),
            Err(e) => e,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_workspace() {
        let (temp, workspace_path) = create_test_workspace();

        assert!(workspace_path.exists());
        assert!(workspace_path.join(".midlight").exists());
        assert!(workspace_path.join(".midlight/objects").exists());
        assert!(workspace_path.join(".midlight/checkpoints").exists());
        assert!(workspace_path.join(".midlight/images").exists());
        assert!(workspace_path.join(".midlight/recovery").exists());

        // Cleanup happens automatically when temp goes out of scope
        drop(temp);
    }

    #[test]
    fn test_create_test_workspace_with_document() {
        let (temp, _workspace_path, doc_path) =
            create_test_workspace_with_document("test.md", "# Hello");

        assert!(doc_path.exists());
        let content = std::fs::read_to_string(&doc_path).unwrap();
        assert_eq!(content, "# Hello");

        drop(temp);
    }

    #[test]
    fn test_sample_markdown() {
        let md = sample_markdown();
        assert!(md.contains("# Test Document"));
        assert!(md.contains("## Section 1"));
    }

    #[test]
    fn test_sample_tiptap_json() {
        let json = sample_tiptap_json();
        assert_eq!(json["type"], "doc");
        assert!(json["content"].is_array());
    }
}
