// File watcher commands - IPC handlers for file watching

use crate::services::file_watcher::FileWatcher;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Runtime;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Registry of file watchers (one per workspace)
pub struct FileWatcherRegistry {
    watchers: HashMap<String, Arc<RwLock<FileWatcher>>>,
}

impl FileWatcherRegistry {
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
        }
    }

    pub fn get(&self, workspace_root: &str) -> Option<Arc<RwLock<FileWatcher>>> {
        self.watchers.get(workspace_root).cloned()
    }

    pub fn insert(&mut self, workspace_root: String, watcher: FileWatcher) {
        self.watchers
            .insert(workspace_root, Arc::new(RwLock::new(watcher)));
    }

    pub fn remove(&mut self, workspace_root: &str) -> Option<Arc<RwLock<FileWatcher>>> {
        self.watchers.remove(workspace_root)
    }
}

impl Default for FileWatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// State for file watchers
pub struct FileWatcherState {
    pub registry: RwLock<FileWatcherRegistry>,
}

impl FileWatcherState {
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(FileWatcherRegistry::new()),
        }
    }
}

impl Default for FileWatcherState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Start watching a workspace for file changes
#[tauri::command]
pub async fn file_watcher_start<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, FileWatcherState>,
    workspace_root: String,
) -> Result<(), String> {
    info!("Starting file watcher for: {}", workspace_root);

    let mut registry = state.registry.write().await;

    // Check if already watching
    if registry.get(&workspace_root).is_some() {
        debug!("File watcher already running for: {}", workspace_root);
        return Ok(());
    }

    // Create and start watcher
    let mut watcher = FileWatcher::new(PathBuf::from(&workspace_root), None);
    watcher.start(app)?;

    registry.insert(workspace_root, watcher);

    Ok(())
}

/// Stop watching a workspace
#[tauri::command]
pub async fn file_watcher_stop<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, FileWatcherState>,
    workspace_root: String,
) -> Result<(), String> {
    info!("Stopping file watcher for: {}", workspace_root);

    let mut registry = state.registry.write().await;

    if let Some(watcher) = registry.remove(&workspace_root) {
        let mut w = watcher.write().await;
        w.stop();
    }

    Ok(())
}

/// Mark a file as being saved (to ignore the change event)
#[tauri::command]
pub async fn file_watcher_mark_saving<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, FileWatcherState>,
    workspace_root: String,
    file_key: String,
) -> Result<(), String> {
    debug!("Marking file as saving: {}", file_key);

    let registry = state.registry.read().await;

    if let Some(watcher) = registry.get(&workspace_root) {
        let w = watcher.read().await;
        w.mark_saving(&file_key);
    }

    Ok(())
}

/// Clear the saving mark after save completes
#[tauri::command]
pub async fn file_watcher_clear_saving<R: Runtime>(
    _app: tauri::AppHandle<R>,
    state: tauri::State<'_, FileWatcherState>,
    workspace_root: String,
    file_key: String,
) -> Result<(), String> {
    debug!("Clearing saving mark for: {}", file_key);

    let registry = state.registry.read().await;

    if let Some(watcher) = registry.get(&workspace_root) {
        let w = watcher.read().await;
        w.clear_saving(&file_key);
    }

    Ok(())
}
