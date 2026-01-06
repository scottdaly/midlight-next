// Midlight Desktop App - Tauri Backend

mod commands;
mod services;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use services::workspace_manager::WorkspaceManagerRegistry;

/// Application state shared across all commands
pub struct AppState {
    pub workspace_registry: Arc<RwLock<WorkspaceManagerRegistry>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            workspace_registry: Arc::new(RwLock::new(WorkspaceManagerRegistry::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("midlight=debug".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting Midlight desktop app");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // File system commands
            commands::fs::get_default_workspace,
            commands::fs::read_dir,
            commands::fs::read_file,
            commands::fs::write_file,
            commands::fs::delete_file,
            commands::fs::rename_file,
            commands::fs::file_exists,
            commands::fs::create_folder,
            commands::fs::create_midlight_file,
            commands::fs::create_new_folder,
            commands::fs::file_duplicate,
            commands::fs::file_trash,
            commands::fs::file_reveal,
            commands::fs::file_copy_to,
            commands::fs::file_move_to,
            // Workspace commands
            commands::workspace::workspace_init,
            commands::workspace::workspace_load_document,
            commands::workspace::workspace_save_document,
            commands::workspace::workspace_get_checkpoints,
            commands::workspace::workspace_restore_checkpoint,
            commands::workspace::workspace_create_bookmark,
            // Version commands
            commands::versions::get_checkpoints,
            commands::versions::restore_checkpoint,
            commands::versions::create_bookmark,
            commands::versions::compare_checkpoints,
            // Image commands
            commands::images::workspace_save_image,
            commands::images::workspace_get_image,
            commands::images::workspace_image_exists,
            commands::images::workspace_delete_image,
            commands::images::workspace_list_images,
            // LLM commands
            commands::llm::llm_chat,
            commands::llm::llm_chat_stream,
            commands::llm::llm_chat_with_tools,
            commands::llm::llm_chat_with_tools_stream,
            commands::llm::llm_get_models,
            commands::llm::llm_get_quota,
            commands::llm::llm_get_status,
            // Agent commands
            commands::agent::agent_execute_tool,
            commands::agent::agent_list_tools,
            // Auth commands
            commands::auth::auth_init,
            commands::auth::auth_signup,
            commands::auth::auth_login,
            commands::auth::auth_logout,
            commands::auth::auth_login_with_google,
            commands::auth::auth_handle_oauth_callback,
            commands::auth::auth_get_user,
            commands::auth::auth_get_subscription,
            commands::auth::auth_get_quota,
            commands::auth::auth_is_authenticated,
            commands::auth::auth_get_state,
            commands::auth::auth_get_access_token,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
