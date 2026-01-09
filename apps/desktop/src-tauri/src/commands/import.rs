// Import commands - IPC handlers for import/export operations

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::oneshot;

use crate::services::import_service::{
    analyze_notion_export, analyze_obsidian_vault, detect_source_type, import_notion_export,
    import_obsidian_vault, CancellationToken, ImportAnalysis, ImportOptions, ImportProgress,
    ImportResult, ImportSourceType, NotionImportOptions,
};

/// Global cancellation token for active import
static ACTIVE_IMPORT_CANCEL: Mutex<Option<Arc<CancellationToken>>> = Mutex::new(None);

/// Select a folder for import using native dialog
#[tauri::command]
pub async fn import_select_folder<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = oneshot::channel();

    app.dialog()
        .file()
        .set_title("Select Import Source")
        .pick_folder(move |result| {
            let _ = tx.send(result);
        });

    match rx.await {
        Ok(Some(path)) => Ok(Some(path.to_string())),
        Ok(None) => Ok(None),
        Err(_) => Err("Dialog channel closed".into()),
    }
}

/// Detect the type of import source (Obsidian, Notion, or Generic)
#[tauri::command]
pub async fn import_detect_source_type(folder_path: String) -> Result<ImportSourceType, String> {
    let path = PathBuf::from(&folder_path);
    detect_source_type(&path).map_err(|e| e.to_string())
}

/// Analyze an Obsidian vault
#[tauri::command]
pub async fn import_analyze_obsidian(vault_path: String) -> Result<ImportAnalysis, String> {
    let path = PathBuf::from(&vault_path);

    // Run analysis in blocking task since it's file I/O heavy
    tokio::task::spawn_blocking(move || analyze_obsidian_vault(&path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

/// Analyze a Notion export
#[tauri::command]
pub async fn import_analyze_notion(export_path: String) -> Result<ImportAnalysis, String> {
    let path = PathBuf::from(&export_path);

    tokio::task::spawn_blocking(move || analyze_notion_export(&path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

/// Import an Obsidian vault
#[tauri::command]
pub async fn import_obsidian<R: Runtime>(
    app: AppHandle<R>,
    analysis_json: String,
    dest_path: String,
    options_json: String,
) -> Result<ImportResult, String> {
    // Parse inputs
    let analysis: ImportAnalysis =
        serde_json::from_str(&analysis_json).map_err(|e| format!("Invalid analysis: {}", e))?;

    let options: ImportOptions =
        serde_json::from_str(&options_json).map_err(|e| format!("Invalid options: {}", e))?;

    let dest = PathBuf::from(&dest_path);

    // Create cancellation token
    let cancel_token = CancellationToken::new();

    // Store cancel token globally
    {
        let mut active = ACTIVE_IMPORT_CANCEL.lock().unwrap();
        *active = Some(cancel_token.clone());
    }

    // Create progress callback that emits events
    let app_handle = app.clone();
    let progress_callback = Box::new(move |progress: ImportProgress| {
        let _ = app_handle.emit("import-progress", &progress);
    });

    // Run import in blocking task
    let result = tokio::task::spawn_blocking(move || {
        import_obsidian_vault(
            &analysis,
            &dest,
            &options,
            Some(progress_callback),
            Some(cancel_token),
        )
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    // Clear cancellation token
    {
        let mut active = ACTIVE_IMPORT_CANCEL.lock().unwrap();
        *active = None;
    }

    result.map_err(|e| e.to_string())
}

/// Import a Notion export
#[tauri::command]
pub async fn import_notion<R: Runtime>(
    app: AppHandle<R>,
    analysis_json: String,
    dest_path: String,
    options_json: String,
) -> Result<ImportResult, String> {
    // Parse inputs
    let analysis: ImportAnalysis =
        serde_json::from_str(&analysis_json).map_err(|e| format!("Invalid analysis: {}", e))?;

    let options: NotionImportOptions =
        serde_json::from_str(&options_json).map_err(|e| format!("Invalid options: {}", e))?;

    let dest = PathBuf::from(&dest_path);

    // Create cancellation token
    let cancel_token = CancellationToken::new();

    // Store cancel token globally
    {
        let mut active = ACTIVE_IMPORT_CANCEL.lock().unwrap();
        *active = Some(cancel_token.clone());
    }

    // Create progress callback
    let app_handle = app.clone();
    let progress_callback = Box::new(move |progress: ImportProgress| {
        let _ = app_handle.emit("import-progress", &progress);
    });

    // Run import
    let result = tokio::task::spawn_blocking(move || {
        import_notion_export(
            &analysis,
            &dest,
            &options,
            Some(progress_callback),
            Some(cancel_token),
        )
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    // Clear cancellation token
    {
        let mut active = ACTIVE_IMPORT_CANCEL.lock().unwrap();
        *active = None;
    }

    result.map_err(|e| e.to_string())
}

/// Cancel an active import
#[tauri::command]
pub async fn import_cancel() -> Result<(), String> {
    let active = ACTIVE_IMPORT_CANCEL.lock().unwrap();
    if let Some(token) = active.as_ref() {
        token.cancel();
        Ok(())
    } else {
        Err("No active import to cancel".into())
    }
}

/// Export current document to PDF using webview print
#[tauri::command]
pub async fn export_pdf<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    // Get the main window
    use tauri::Manager;
    let window = app
        .get_webview_window("main")
        .ok_or("Could not get main window")?;

    // Use the print API
    // Note: Tauri 2 may have different print API, this is a placeholder
    // The actual implementation depends on Tauri's webview capabilities
    window.print().map_err(|e| format!("Print failed: {}", e))?;

    Ok(true)
}
