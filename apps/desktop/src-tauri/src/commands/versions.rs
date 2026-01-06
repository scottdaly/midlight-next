// Version history commands

use crate::services::checkpoint_manager::Checkpoint;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use super::workspace::SaveResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    pub additions: Vec<String>,
    pub deletions: Vec<String>,
    #[serde(rename = "changeCount")]
    pub change_count: u32,
}

#[tauri::command]
pub async fn get_checkpoints(
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
pub async fn restore_checkpoint(
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
pub async fn create_bookmark(
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
pub async fn compare_checkpoints(
    workspace_root: String,
    file_path: String,
    checkpoint_id_a: String,
    checkpoint_id_b: String,
    state: State<'_, AppState>,
) -> Result<DiffResult, String> {
    let registry = state.workspace_registry.read().await;

    if let Some(manager) = registry.get(&workspace_root) {
        manager
            .compare_checkpoints(&file_path, &checkpoint_id_a, &checkpoint_id_b)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("Workspace not initialized".to_string())
    }
}
