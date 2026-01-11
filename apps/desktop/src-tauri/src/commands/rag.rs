// RAG Commands - Tauri command handlers for RAG operations
//
// Exposes the RAG service functionality to the frontend via IPC.

use crate::services::rag_service::{RAGService, SearchOptions};
use crate::services::vector_store::{IndexStatus, SearchResult};
use tauri::AppHandle;
use tauri::Manager;
use tokio::sync::OnceCell;
use tracing::{debug, info};

// ============================================================================
// Singleton Service
// ============================================================================

/// Global RAG service instance - initialized lazily on first use
static RAG_SERVICE: OnceCell<RAGService> = OnceCell::const_new();

/// Get or initialize the RAG service
async fn get_service(app: &AppHandle) -> Result<&'static RAGService, String> {
    RAG_SERVICE
        .get_or_try_init(|| async {
            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Failed to get app data dir: {}", e))?;

            // Create RAG directory
            let rag_dir = app_data.join("rag");
            std::fs::create_dir_all(&rag_dir)
                .map_err(|e| format!("Failed to create RAG directory: {}", e))?;

            let db_path = rag_dir.join("vectors.db");
            info!("Initializing RAG service at {:?}", db_path);

            RAGService::new(db_path)
        })
        .await
}

// ============================================================================
// Commands
// ============================================================================

/// Index a project for semantic search
#[tauri::command]
pub async fn rag_index_project(
    app: AppHandle,
    project_path: String,
    auth_token: String,
    force: Option<bool>,
) -> Result<IndexStatus, String> {
    debug!("rag_index_project: {}", project_path);

    let service = get_service(&app).await?;

    service
        .index_project(&project_path, &auth_token, force.unwrap_or(false))
        .await
        .map_err(|e| e.message)
}

/// Search for relevant document chunks
#[tauri::command]
pub async fn rag_search(
    app: AppHandle,
    query: String,
    auth_token: String,
    top_k: Option<u32>,
    min_score: Option<f32>,
    project_paths: Option<Vec<String>>,
) -> Result<Vec<SearchResult>, String> {
    debug!("rag_search: {}", query);

    let service = get_service(&app).await?;

    let options = SearchOptions {
        top_k,
        min_score,
        project_paths,
    };

    service
        .search(&query, &auth_token, Some(options))
        .await
        .map_err(|e| e.message)
}

/// Get index status for projects
#[tauri::command]
pub async fn rag_get_status(
    app: AppHandle,
    project_path: Option<String>,
) -> Result<Vec<IndexStatus>, String> {
    debug!("rag_get_status: {:?}", project_path);

    let service = get_service(&app).await?;

    service
        .get_status(project_path.as_deref())
        .await
        .map_err(|e| e.message)
}

/// Delete index for a project
#[tauri::command]
pub async fn rag_delete_index(app: AppHandle, project_path: String) -> Result<(), String> {
    debug!("rag_delete_index: {}", project_path);

    let service = get_service(&app).await?;

    service.delete_index(&project_path).await.map_err(|e| e.message)
}

/// Index a single file (for real-time updates)
#[tauri::command]
pub async fn rag_index_file(
    app: AppHandle,
    project_path: String,
    file_path: String,
    auth_token: String,
) -> Result<(), String> {
    debug!("rag_index_file: {} in {}", file_path, project_path);

    let service = get_service(&app).await?;

    service
        .index_file(&project_path, &file_path, &auth_token)
        .await
        .map_err(|e| e.message)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    // Note: Command tests require a full Tauri app context
    // These would be integration tests rather than unit tests
}
