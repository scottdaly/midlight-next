// Midlight Desktop App - Tauri Backend
#![allow(clippy::manual_strip)]
#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::bind_instead_of_map)]

mod commands;
#[cfg(target_os = "macos")]
mod menu;
mod services;
pub mod traits;

#[cfg(test)]
mod test_utils;

use std::sync::Arc;
use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tokio::sync::RwLock;

use commands::error_reporter::ErrorReporterState;
use commands::file_watcher::FileWatcherState;
use commands::recovery::RecoveryState;
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
        .manage(RecoveryState::new())
        .manage(FileWatcherState::new())
        .manage(ErrorReporterState::default())
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
            commands::auth::auth_forgot_password,
            commands::auth::auth_reset_password,
            commands::auth::auth_update_profile,
            // Subscription commands
            commands::auth::subscription_get_prices,
            commands::auth::subscription_create_checkout,
            commands::auth::subscription_create_portal,
            // Import commands
            commands::import::import_select_folder,
            commands::import::import_detect_source_type,
            commands::import::import_analyze_obsidian,
            commands::import::import_analyze_notion,
            commands::import::import_obsidian,
            commands::import::import_notion,
            commands::import::import_cancel,
            // DOCX import commands
            commands::import::import_select_docx_file,
            commands::import::import_analyze_docx,
            commands::import::import_docx_file,
            // Export commands
            commands::import::export_pdf,
            commands::export::export_select_save_path,
            commands::export::export_to_docx,
            // Recovery commands
            commands::recovery::recovery_check,
            commands::recovery::recovery_write_wal,
            commands::recovery::recovery_clear_wal,
            commands::recovery::recovery_has_recovery,
            commands::recovery::recovery_get_content,
            commands::recovery::recovery_discard,
            commands::recovery::recovery_discard_all,
            commands::recovery::recovery_has_unique_content,
            // File watcher commands
            commands::file_watcher::file_watcher_start,
            commands::file_watcher::file_watcher_stop,
            commands::file_watcher::file_watcher_mark_saving,
            commands::file_watcher::file_watcher_clear_saving,
            // Error reporter commands
            commands::error_reporter::error_reporter_set_enabled,
            commands::error_reporter::error_reporter_get_status,
            commands::error_reporter::error_reporter_report,
            // System commands
            commands::system::show_in_folder,
            commands::system::open_external,
            commands::system::get_app_version,
            commands::system::get_platform_info,
            // Update commands
            commands::updates::check_for_updates,
            commands::updates::download_and_install_update,
            commands::updates::get_current_version,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }

            #[cfg(target_os = "macos")]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    // Force the window to have a shadow and proper title bar settings
                    let _ = window.set_shadow(true);
                }

                // Set up native macOS menu
                let menu = menu::create_menu(app.handle())?;
                app.set_menu(menu)?;
            }

            // Set up system tray icon
            let show_item = MenuItemBuilder::with_id("show", "Show Midlight").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_menu_event(|_app, _event| {
            #[cfg(target_os = "macos")]
            menu::handle_menu_event(_app, _event.id().as_ref());
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
