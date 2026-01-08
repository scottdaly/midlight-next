// Error Reporter commands - IPC handlers for error reporting

use crate::services::error_reporter::{ErrorCategory, ErrorReporter};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Runtime;

// ============================================================================
// State
// ============================================================================

/// State for error reporter (shared across all commands)
pub struct ErrorReporterState {
    pub reporter: Arc<ErrorReporter>,
}

impl ErrorReporterState {
    pub fn new(app_version: &str) -> Self {
        Self {
            reporter: Arc::new(ErrorReporter::new(app_version)),
        }
    }
}

impl Default for ErrorReporterState {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ErrorReporterStatus {
    pub enabled: bool,
    pub reports_this_session: u32,
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Enable or disable error reporting
#[tauri::command]
pub async fn error_reporter_set_enabled<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, ErrorReporterState>,
    enabled: bool,
) -> Result<(), String> {
    state.reporter.set_enabled(enabled);
    Ok(())
}

/// Get error reporting status
#[tauri::command]
pub async fn error_reporter_get_status<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, ErrorReporterState>,
) -> Result<ErrorReporterStatus, String> {
    Ok(ErrorReporterStatus {
        enabled: state.reporter.is_enabled(),
        reports_this_session: state.reporter.reports_count(),
    })
}

/// Report an error manually
#[tauri::command]
pub async fn error_reporter_report<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, ErrorReporterState>,
    category: String,
    error_type: String,
    message: String,
    context: Option<HashMap<String, String>>,
) -> Result<(), String> {
    let cat = match category.as_str() {
        "import" => ErrorCategory::Import,
        "export" => ErrorCategory::Export,
        "file_system" => ErrorCategory::FileSystem,
        "editor" => ErrorCategory::Editor,
        "llm" => ErrorCategory::Llm,
        "auth" => ErrorCategory::Auth,
        "recovery" => ErrorCategory::Recovery,
        _ => ErrorCategory::Unknown,
    };

    state
        .reporter
        .report(cat, &error_type, &message, context)
        .await;

    Ok(())
}
