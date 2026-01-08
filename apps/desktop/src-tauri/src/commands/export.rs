// Export commands for Tauri
// Handles DOCX export operations

use crate::services::docx_export::{tiptap_to_docx, TiptapDocument};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub success: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

// ============================================================================
// Commands
// ============================================================================

/// Opens a save dialog for selecting the export destination
#[tauri::command]
pub async fn export_select_save_path<R: Runtime>(
    app: AppHandle<R>,
    default_name: String,
    file_type: String,
) -> Result<Option<String>, String> {
    let (extension, filter_name) = match file_type.as_str() {
        "docx" => ("docx", "Word Document"),
        "pdf" => ("pdf", "PDF Document"),
        _ => return Err(format!("Unsupported file type: {}", file_type)),
    };

    let (tx, rx) = oneshot::channel();

    app.dialog()
        .file()
        .set_title(&format!("Export as {}", filter_name))
        .set_file_name(&format!("{}.{}", default_name, extension))
        .add_filter(filter_name, &[extension])
        .save_file(move |result| {
            let _ = tx.send(result);
        });

    match rx.await {
        Ok(Some(path)) => Ok(Some(path.to_string())),
        Ok(None) => Ok(None),
        Err(_) => Err("Dialog was cancelled".to_string()),
    }
}

/// Exports the document to DOCX format
#[tauri::command]
pub async fn export_to_docx<R: Runtime>(
    app: AppHandle<R>,
    content: TiptapDocument,
    output_path: String,
) -> Result<ExportResult, String> {
    let app_handle = app.clone();

    // Run export in a blocking task to avoid blocking the async runtime
    let result = tokio::task::spawn_blocking(move || {
        tiptap_to_docx(&content, |progress| {
            let _ = app_handle.emit("export:progress", &progress);
        })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(bytes) => {
            // Write to file
            let path = PathBuf::from(&output_path);

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            }

            std::fs::write(&path, &bytes)
                .map_err(|e| format!("Failed to write file: {}", e))?;

            Ok(ExportResult {
                success: true,
                path: Some(output_path),
                error: None,
            })
        }
        Err(e) => Ok(ExportResult {
            success: false,
            path: None,
            error: Some(e),
        }),
    }
}
