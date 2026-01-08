// Recovery commands - IPC handlers for crash recovery

use crate::services::recovery_manager::{RecoveryFile, RecoveryManager};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Runtime;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Registry of recovery managers (one per workspace)
pub struct RecoveryManagerRegistry {
    managers: HashMap<String, Arc<RecoveryManager>>,
}

impl RecoveryManagerRegistry {
    pub fn new() -> Self {
        Self {
            managers: HashMap::new(),
        }
    }

    pub async fn get_or_create(&mut self, workspace_root: &str) -> Arc<RecoveryManager> {
        if let Some(manager) = self.managers.get(workspace_root) {
            return manager.clone();
        }

        let manager = Arc::new(RecoveryManager::new(PathBuf::from(workspace_root)));

        // Initialize the recovery directory
        if let Err(e) = manager.init().await {
            tracing::warn!("Failed to initialize recovery manager: {}", e);
        }

        self.managers
            .insert(workspace_root.to_string(), manager.clone());
        manager
    }
}

impl Default for RecoveryManagerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// State for recovery managers
pub struct RecoveryState {
    pub registry: RwLock<RecoveryManagerRegistry>,
}

impl RecoveryState {
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(RecoveryManagerRegistry::new()),
        }
    }
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Check for recovery files on startup
/// Returns list of files with unsaved changes
#[tauri::command]
pub async fn recovery_check<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
) -> Result<Vec<RecoveryFile>, String> {
    debug!("Checking for recovery files in: {}", workspace_root);

    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    manager.check_for_recovery().await
}

/// Write WAL file for a document
/// Called periodically while editing to save recovery data
#[tauri::command]
pub async fn recovery_write_wal<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
    file_key: String,
    content: String,
) -> Result<bool, String> {
    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    manager.write_wal(&file_key, &content).await
}

/// Clear WAL file after successful save
#[tauri::command]
pub async fn recovery_clear_wal<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
    file_key: String,
) -> Result<(), String> {
    debug!("Clearing WAL for: {}", file_key);

    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    manager.clear_wal(&file_key).await
}

/// Check if a specific file has recovery available
#[tauri::command]
pub async fn recovery_has_recovery<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
    file_key: String,
) -> Result<bool, String> {
    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    Ok(manager.has_recovery(&file_key).await)
}

/// Get recovery content for a specific file
#[tauri::command]
pub async fn recovery_get_content<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
    file_key: String,
) -> Result<Option<String>, String> {
    debug!("Getting recovery content for: {}", file_key);

    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    manager.get_recovery_content(&file_key).await
}

/// Discard recovery for a specific file
#[tauri::command]
pub async fn recovery_discard<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
    file_key: String,
) -> Result<(), String> {
    info!("Discarding recovery for: {}", file_key);

    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    manager.discard_recovery(&file_key).await
}

/// Discard all recovery files for a workspace
#[tauri::command]
pub async fn recovery_discard_all<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
) -> Result<(), String> {
    info!("Discarding all recovery files in: {}", workspace_root);

    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    manager.discard_all_recovery().await
}

/// Check if recovery content differs from current file content
#[tauri::command]
pub async fn recovery_has_unique_content<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, RecoveryState>,
    workspace_root: String,
    file_key: String,
    current_content: String,
) -> Result<bool, String> {
    let mut registry = state.registry.write().await;
    let manager = registry.get_or_create(&workspace_root).await;

    manager
        .has_unique_recovery(&file_key, &current_content)
        .await
}
