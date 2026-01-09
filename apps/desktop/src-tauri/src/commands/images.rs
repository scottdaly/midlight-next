// Image commands - Upload, retrieve, and manage images

use crate::services::image_manager::ImageManager;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUploadResult {
    #[serde(rename = "refId")]
    pub ref_id: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Save an image to the workspace
#[tauri::command]
pub async fn workspace_save_image(
    workspace_root: String,
    data_url: String,
    original_name: Option<String>,
) -> Result<ImageUploadResult, String> {
    let manager = ImageManager::new(Path::new(&workspace_root));
    manager.init().await.map_err(|e| e.to_string())?;

    match manager
        .store_image(&data_url, original_name.as_deref())
        .await
    {
        Ok(ref_id) => Ok(ImageUploadResult {
            ref_id,
            success: true,
            error: None,
        }),
        Err(e) => Ok(ImageUploadResult {
            ref_id: String::new(),
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

/// Get an image as a data URL
#[tauri::command]
pub async fn workspace_get_image(workspace_root: String, ref_id: String) -> Result<String, String> {
    let manager = ImageManager::new(Path::new(&workspace_root));
    manager
        .get_image_data_url(&ref_id)
        .await
        .map_err(|e| e.to_string())
}

/// Check if an image exists
#[tauri::command]
pub async fn workspace_image_exists(
    workspace_root: String,
    ref_id: String,
) -> Result<bool, String> {
    let manager = ImageManager::new(Path::new(&workspace_root));
    Ok(manager.exists(&ref_id))
}

/// Delete an image
#[tauri::command]
pub async fn workspace_delete_image(workspace_root: String, ref_id: String) -> Result<(), String> {
    let manager = ImageManager::new(Path::new(&workspace_root));
    manager.delete(&ref_id).await.map_err(|e| e.to_string())
}

/// List all images in the workspace
#[tauri::command]
pub async fn workspace_list_images(workspace_root: String) -> Result<Vec<String>, String> {
    let manager = ImageManager::new(Path::new(&workspace_root));
    manager.list_images().await.map_err(|e| e.to_string())
}
