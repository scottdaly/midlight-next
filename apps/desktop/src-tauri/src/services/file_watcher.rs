// File Watcher - Monitors workspace files for external changes
//
// Uses the `notify` crate for native file system events.
// Debounces events and distinguishes between app-initiated and external changes.

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};
use tracing::{debug, error, info};

// ============================================================================
// Types
// ============================================================================

/// File change event sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    /// Type of change
    pub change_type: String, // "modify", "create", "delete", "rename"
    /// Relative path from workspace root (file key)
    pub file_key: String,
    /// Timestamp as ISO string
    pub timestamp: String,
}

/// Pending event for debouncing
#[derive(Debug, Clone)]
struct PendingEvent {
    change_type: String,
    #[allow(dead_code)] // May be needed for event timing analysis
    first_seen: Instant,
    last_seen: Instant,
}

/// File watcher configuration
#[derive(Debug, Clone)]
pub struct FileWatcherConfig {
    /// Debounce delay
    pub debounce_ms: u64,
    /// Patterns to ignore
    pub ignored_patterns: Vec<String>,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 500,
            ignored_patterns: vec![
                ".git".to_string(),
                ".midlight".to_string(),
                "node_modules".to_string(),
                ".DS_Store".to_string(),
                "Thumbs.db".to_string(),
            ],
        }
    }
}

// ============================================================================
// File Watcher
// ============================================================================

pub struct FileWatcher {
    workspace_root: PathBuf,
    config: FileWatcherConfig,
    /// Files currently being saved by the app
    saving_files: Arc<Mutex<HashSet<PathBuf>>>,
    /// Recent saves with grace period
    recent_saves: Arc<Mutex<HashMap<PathBuf, Instant>>>,
    /// Pending events for debouncing
    pending_events: Arc<Mutex<HashMap<PathBuf, PendingEvent>>>,
    /// Watcher handle
    watcher: Option<RecommendedWatcher>,
    /// Stop signal
    stop_tx: Option<Sender<()>>,
}

impl FileWatcher {
    /// Create a new file watcher for the given workspace
    pub fn new(workspace_root: PathBuf, config: Option<FileWatcherConfig>) -> Self {
        Self {
            workspace_root,
            config: config.unwrap_or_default(),
            saving_files: Arc::new(Mutex::new(HashSet::new())),
            recent_saves: Arc::new(Mutex::new(HashMap::new())),
            pending_events: Arc::new(Mutex::new(HashMap::new())),
            watcher: None,
            stop_tx: None,
        }
    }

    /// Start watching the workspace
    pub fn start<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        if self.watcher.is_some() {
            return Ok(()); // Already watching
        }

        let (tx, rx) = channel::<notify::Result<Event>>();
        let (stop_tx, stop_rx) = channel::<()>();

        // Create watcher
        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default().with_poll_interval(Duration::from_millis(100)),
        )
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        self.watcher = Some(watcher);
        self.stop_tx = Some(stop_tx);

        // Start watching
        if let Some(ref mut w) = self.watcher {
            w.watch(&self.workspace_root, RecursiveMode::Recursive)
                .map_err(|e| format!("Failed to watch directory: {}", e))?;
        }

        info!("File watcher started for: {:?}", self.workspace_root);

        // Spawn event processing thread
        let workspace_root = self.workspace_root.clone();
        let config = self.config.clone();
        let saving_files = self.saving_files.clone();
        let recent_saves = self.recent_saves.clone();
        let pending_events = self.pending_events.clone();

        std::thread::spawn(move || {
            Self::event_loop(
                rx,
                stop_rx,
                app,
                workspace_root,
                config,
                saving_files,
                recent_saves,
                pending_events,
            );
        });

        Ok(())
    }

    /// Stop watching
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }

        if let Some(mut watcher) = self.watcher.take() {
            let _ = watcher.unwatch(&self.workspace_root);
        }

        // Clear state
        if let Ok(mut saving) = self.saving_files.lock() {
            saving.clear();
        }
        if let Ok(mut recent) = self.recent_saves.lock() {
            recent.clear();
        }
        if let Ok(mut pending) = self.pending_events.lock() {
            pending.clear();
        }

        info!("File watcher stopped");
    }

    /// Mark a file as being saved by the app
    pub fn mark_saving(&self, file_key: &str) {
        let path = self.workspace_root.join(file_key);
        if let Ok(mut saving) = self.saving_files.lock() {
            saving.insert(path);
        }
    }

    /// Clear the saving mark after save completes
    pub fn clear_saving(&self, file_key: &str) {
        let path = self.workspace_root.join(file_key);
        if let Ok(mut saving) = self.saving_files.lock() {
            saving.remove(&path);
        }
        // Add to recent saves with grace period
        if let Ok(mut recent) = self.recent_saves.lock() {
            recent.insert(path, Instant::now());
        }
    }

    /// Event processing loop
    fn event_loop<R: Runtime>(
        rx: Receiver<notify::Result<Event>>,
        stop_rx: Receiver<()>,
        app: AppHandle<R>,
        workspace_root: PathBuf,
        config: FileWatcherConfig,
        saving_files: Arc<Mutex<HashSet<PathBuf>>>,
        recent_saves: Arc<Mutex<HashMap<PathBuf, Instant>>>,
        pending_events: Arc<Mutex<HashMap<PathBuf, PendingEvent>>>,
    ) {
        let debounce_duration = Duration::from_millis(config.debounce_ms);
        let grace_period = Duration::from_secs(1);
        let mut last_flush = Instant::now();

        loop {
            // Check for stop signal
            if stop_rx.try_recv().is_ok() {
                break;
            }

            // Process incoming events (non-blocking with timeout)
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    Self::handle_event(
                        &event,
                        &workspace_root,
                        &config,
                        &saving_files,
                        &recent_saves,
                        &pending_events,
                        grace_period,
                    );
                }
                Ok(Err(e)) => {
                    error!("Watch error: {:?}", e);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // No event, continue
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }

            // Flush pending events periodically
            if last_flush.elapsed() > Duration::from_millis(100) {
                Self::flush_pending(&app, &workspace_root, &pending_events, debounce_duration);
                last_flush = Instant::now();
            }

            // Clean up old recent saves
            if let Ok(mut recent) = recent_saves.lock() {
                recent.retain(|_, time| time.elapsed() < grace_period);
            }
        }

        debug!("File watcher event loop ended");
    }

    /// Handle a single file system event
    fn handle_event(
        event: &Event,
        workspace_root: &Path,
        config: &FileWatcherConfig,
        saving_files: &Arc<Mutex<HashSet<PathBuf>>>,
        recent_saves: &Arc<Mutex<HashMap<PathBuf, Instant>>>,
        pending_events: &Arc<Mutex<HashMap<PathBuf, PendingEvent>>>,
        grace_period: Duration,
    ) {
        for path in &event.paths {
            // Skip if path is not under workspace
            if !path.starts_with(workspace_root) {
                continue;
            }

            // Get relative path
            let relative = match path.strip_prefix(workspace_root) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Check if path should be ignored
            let path_str = relative.to_string_lossy();
            if config.ignored_patterns.iter().any(|p| path_str.contains(p)) {
                continue;
            }

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Check if this is an app-initiated change
            if let Ok(saving) = saving_files.lock() {
                if saving.contains(path) {
                    debug!("Ignoring app-initiated change: {:?}", path);
                    continue;
                }
            }

            // Check grace period for recent saves
            if let Ok(recent) = recent_saves.lock() {
                if let Some(save_time) = recent.get(path) {
                    if save_time.elapsed() < grace_period {
                        debug!("Ignoring change within grace period: {:?}", path);
                        continue;
                    }
                }
            }

            // Determine change type
            let change_type = match event.kind {
                EventKind::Create(_) => "create",
                EventKind::Modify(_) => "modify",
                EventKind::Remove(_) => "delete",
                _ => continue, // Ignore other events
            };

            // Add to pending events
            let now = Instant::now();
            if let Ok(mut pending) = pending_events.lock() {
                pending
                    .entry(path.clone())
                    .and_modify(|e| {
                        e.last_seen = now;
                        // Escalate: modify -> delete becomes delete
                        if change_type == "delete" {
                            e.change_type = "delete".to_string();
                        }
                    })
                    .or_insert(PendingEvent {
                        change_type: change_type.to_string(),
                        first_seen: now,
                        last_seen: now,
                    });
            }
        }
    }

    /// Flush pending events that have stabilized
    fn flush_pending<R: Runtime>(
        app: &AppHandle<R>,
        workspace_root: &Path,
        pending_events: &Arc<Mutex<HashMap<PathBuf, PendingEvent>>>,
        debounce_duration: Duration,
    ) {
        let now = Instant::now();
        let mut to_emit = Vec::new();

        if let Ok(mut pending) = pending_events.lock() {
            let ready: Vec<PathBuf> = pending
                .iter()
                .filter(|(_, e)| now - e.last_seen > debounce_duration)
                .map(|(p, _)| p.clone())
                .collect();

            for path in ready {
                if let Some(event) = pending.remove(&path) {
                    let file_key = path
                        .strip_prefix(workspace_root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    to_emit.push(FileChangeEvent {
                        change_type: event.change_type,
                        file_key,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }
            }
        }

        // Emit events
        for event in to_emit {
            debug!("Emitting file change: {:?}", event);
            if let Err(e) = app.emit("file-watcher:change", &event) {
                error!("Failed to emit file change event: {}", e);
            }
        }
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}
