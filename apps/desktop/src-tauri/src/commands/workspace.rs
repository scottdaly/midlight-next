// Workspace commands - Document loading, saving, and versioning

use crate::services::checkpoint_manager::Checkpoint;
use crate::services::workspace_manager::ProjectInfo;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedDocument {
    pub json: Value,
    pub sidecar: Value,
    #[serde(rename = "hasRecovery")]
    pub has_recovery: bool,
    #[serde(rename = "recoveryTime")]
    pub recovery_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveResult {
    pub success: bool,
    #[serde(rename = "checkpointId")]
    pub checkpoint_id: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn workspace_init(
    workspace_root: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut registry = state.workspace_registry.write().await;
    registry
        .get_or_create(&workspace_root)
        .await
        .map_err(|e| e.to_string())?
        .init()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn workspace_load_document(
    workspace_root: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<LoadedDocument, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        manager
            .load_document(&file_path)
            .await
            .map_err(|e| e.to_string())
    } else {
        // Auto-init workspace if not exists
        drop(registry);
        let mut registry = state.workspace_registry.write().await;
        let manager = registry
            .get_or_create(&workspace_root)
            .await
            .map_err(|e| e.to_string())?;
        manager.init().await.map_err(|e| e.to_string())?;
        manager
            .load_document(&file_path)
            .await
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn workspace_save_document(
    workspace_root: String,
    file_path: String,
    json: Value,
    trigger: String,
    state: State<'_, AppState>,
) -> Result<SaveResult, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        manager
            .save_document(&file_path, json, &trigger)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("Workspace not initialized".to_string())
    }
}

#[tauri::command]
pub async fn workspace_get_checkpoints(
    workspace_root: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<Checkpoint>, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        manager
            .get_checkpoints(&file_path)
            .await
            .map_err(|e| e.to_string())
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
pub async fn workspace_restore_checkpoint(
    workspace_root: String,
    file_path: String,
    checkpoint_id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        manager
            .restore_checkpoint(&file_path, &checkpoint_id)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("Workspace not initialized".to_string())
    }
}

#[tauri::command]
pub async fn workspace_create_bookmark(
    workspace_root: String,
    file_path: String,
    json: Value,
    label: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<SaveResult, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        manager
            .create_bookmark(&file_path, json, &label, description.as_deref())
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("Workspace not initialized".to_string())
    }
}

#[tauri::command]
pub async fn workspace_scan_projects(
    workspace_root: String,
    state: State<'_, AppState>,
) -> Result<Vec<ProjectInfo>, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        manager.scan_projects().map_err(|e| e.to_string())
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
pub async fn workspace_is_project(
    workspace_root: String,
    relative_path: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        Ok(manager.is_project(&relative_path))
    } else {
        Ok(false)
    }
}
