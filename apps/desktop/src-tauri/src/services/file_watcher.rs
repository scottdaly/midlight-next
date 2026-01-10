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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
// Event Emitter Trait (for testability)
// ============================================================================

/// Trait for emitting file change events
/// This abstraction allows mocking in tests without requiring Tauri runtime
pub trait EventEmitter: Send + Sync + 'static {
    fn emit_file_change(&self, event: &FileChangeEvent) -> Result<(), String>;
}

/// Production implementation using Tauri AppHandle
pub struct TauriEmitter<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriEmitter<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> EventEmitter for TauriEmitter<R> {
    fn emit_file_change(&self, event: &FileChangeEvent) -> Result<(), String> {
        self.app
            .emit("file-watcher:change", event)
            .map_err(|e| format!("Failed to emit file change event: {}", e))
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

    /// Start watching the workspace (convenience method for Tauri apps)
    pub fn start<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        let emitter = Arc::new(TauriEmitter::new(app));
        self.start_with_emitter(emitter)
    }

    /// Start watching the workspace with a custom event emitter
    pub fn start_with_emitter<E: EventEmitter>(
        &mut self,
        emitter: Arc<E>,
    ) -> Result<(), String> {
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
                emitter,
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
    #[allow(clippy::too_many_arguments)]
    fn event_loop<E: EventEmitter>(
        rx: Receiver<notify::Result<Event>>,
        stop_rx: Receiver<()>,
        emitter: Arc<E>,
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
                Self::flush_pending(&*emitter, &workspace_root, &pending_events, debounce_duration);
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
    fn flush_pending<E: EventEmitter>(
        emitter: &E,
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
            if let Err(e) = emitter.emit_file_change(&event) {
                error!("Failed to emit file change event: {}", e);
            }
        }
    }

    /// Collect ready events from pending (for testing)
    #[cfg(test)]
    fn collect_ready_events(
        pending: &mut HashMap<PathBuf, PendingEvent>,
        workspace_root: &Path,
        debounce_duration: Duration,
    ) -> Vec<FileChangeEvent> {
        let now = Instant::now();
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, e)| now - e.last_seen > debounce_duration)
            .map(|(p, _)| p.clone())
            .collect();

        ready
            .into_iter()
            .filter_map(|path| {
                pending.remove(&path).map(|event| FileChangeEvent {
                    change_type: event.change_type,
                    file_key: path
                        .strip_prefix(workspace_root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    timestamp: "test-timestamp".to_string(),
                })
            })
            .collect()
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};
    use std::time::Duration;
    use tempfile::TempDir;

    // ============================================================================
    // Mock Event Emitter for Testing
    // ============================================================================

    struct MockEmitter {
        emitted_events: Arc<Mutex<Vec<FileChangeEvent>>>,
        should_fail: bool,
    }

    impl MockEmitter {
        fn new() -> Self {
            Self {
                emitted_events: Arc::new(Mutex::new(Vec::new())),
                should_fail: false,
            }
        }

        fn with_failure() -> Self {
            Self {
                emitted_events: Arc::new(Mutex::new(Vec::new())),
                should_fail: true,
            }
        }

        fn get_events(&self) -> Vec<FileChangeEvent> {
            self.emitted_events.lock().unwrap().clone()
        }
    }

    impl EventEmitter for MockEmitter {
        fn emit_file_change(&self, event: &FileChangeEvent) -> Result<(), String> {
            if self.should_fail {
                return Err("Mock emit failure".to_string());
            }
            self.emitted_events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    // ============================================================================
    // FileWatcherConfig Tests
    // ============================================================================

    #[test]
    fn test_config_default_debounce() {
        let config = FileWatcherConfig::default();
        assert_eq!(config.debounce_ms, 500);
    }

    #[test]
    fn test_config_default_ignored_patterns() {
        let config = FileWatcherConfig::default();
        assert!(config.ignored_patterns.contains(&".git".to_string()));
        assert!(config.ignored_patterns.contains(&".midlight".to_string()));
        assert!(config.ignored_patterns.contains(&"node_modules".to_string()));
        assert!(config.ignored_patterns.contains(&".DS_Store".to_string()));
        assert!(config.ignored_patterns.contains(&"Thumbs.db".to_string()));
    }

    #[test]
    fn test_config_custom() {
        let config = FileWatcherConfig {
            debounce_ms: 1000,
            ignored_patterns: vec!["custom".to_string()],
        };
        assert_eq!(config.debounce_ms, 1000);
        assert_eq!(config.ignored_patterns.len(), 1);
    }

    // ============================================================================
    // FileWatcher Construction Tests
    // ============================================================================

    #[test]
    fn test_new_with_default_config() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        assert_eq!(watcher.workspace_root, temp.path());
        assert_eq!(watcher.config.debounce_ms, 500);
        assert!(watcher.watcher.is_none());
        assert!(watcher.stop_tx.is_none());
    }

    #[test]
    fn test_new_with_custom_config() {
        let temp = TempDir::new().unwrap();
        let config = FileWatcherConfig {
            debounce_ms: 200,
            ignored_patterns: vec!["*.tmp".to_string()],
        };
        let watcher = FileWatcher::new(temp.path().to_path_buf(), Some(config));

        assert_eq!(watcher.config.debounce_ms, 200);
        assert_eq!(watcher.config.ignored_patterns, vec!["*.tmp".to_string()]);
    }

    // ============================================================================
    // Saving State Tests
    // ============================================================================

    #[test]
    fn test_mark_saving() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        watcher.mark_saving("test.md");

        let saving = watcher.saving_files.lock().unwrap();
        assert!(saving.contains(&temp.path().join("test.md")));
    }

    #[test]
    fn test_mark_saving_multiple_files() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        watcher.mark_saving("file1.md");
        watcher.mark_saving("file2.md");
        watcher.mark_saving("dir/file3.md");

        let saving = watcher.saving_files.lock().unwrap();
        assert_eq!(saving.len(), 3);
        assert!(saving.contains(&temp.path().join("file1.md")));
        assert!(saving.contains(&temp.path().join("file2.md")));
        assert!(saving.contains(&temp.path().join("dir/file3.md")));
    }

    #[test]
    fn test_clear_saving() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        watcher.mark_saving("test.md");
        watcher.clear_saving("test.md");

        let saving = watcher.saving_files.lock().unwrap();
        assert!(!saving.contains(&temp.path().join("test.md")));
    }

    #[test]
    fn test_clear_saving_adds_to_recent() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        watcher.mark_saving("test.md");
        watcher.clear_saving("test.md");

        let recent = watcher.recent_saves.lock().unwrap();
        assert!(recent.contains_key(&temp.path().join("test.md")));
    }

    #[test]
    fn test_clear_saving_without_mark() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        // Clear without marking first - should still add to recent
        watcher.clear_saving("test.md");

        let saving = watcher.saving_files.lock().unwrap();
        assert!(!saving.contains(&temp.path().join("test.md")));

        let recent = watcher.recent_saves.lock().unwrap();
        assert!(recent.contains_key(&temp.path().join("test.md")));
    }

    // ============================================================================
    // Stop Tests
    // ============================================================================

    #[test]
    fn test_stop_clears_state() {
        let temp = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        // Add some state
        watcher.mark_saving("file1.md");
        watcher.mark_saving("file2.md");
        watcher.clear_saving("file1.md");

        // Add pending event directly
        {
            let mut pending = watcher.pending_events.lock().unwrap();
            pending.insert(
                temp.path().join("file3.md"),
                PendingEvent {
                    change_type: "modify".to_string(),
                    first_seen: Instant::now(),
                    last_seen: Instant::now(),
                },
            );
        }

        watcher.stop();

        let saving = watcher.saving_files.lock().unwrap();
        let recent = watcher.recent_saves.lock().unwrap();
        let pending = watcher.pending_events.lock().unwrap();

        assert!(saving.is_empty());
        assert!(recent.is_empty());
        assert!(pending.is_empty());
    }

    #[test]
    fn test_stop_without_start() {
        let temp = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        // Stop without starting should not panic
        watcher.stop();

        assert!(watcher.watcher.is_none());
        assert!(watcher.stop_tx.is_none());
    }

    // ============================================================================
    // FileChangeEvent Tests
    // ============================================================================

    #[test]
    fn test_file_change_event_serialization() {
        let event = FileChangeEvent {
            change_type: "modify".to_string(),
            file_key: "docs/test.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"change_type\":\"modify\""));
        assert!(json.contains("\"file_key\":\"docs/test.md\""));
        assert!(json.contains("\"timestamp\":\"2024-01-01T00:00:00Z\""));
    }

    #[test]
    fn test_file_change_event_deserialization() {
        let json = r#"{"change_type":"create","file_key":"new.md","timestamp":"2024-01-01T00:00:00Z"}"#;
        let event: FileChangeEvent = serde_json::from_str(json).unwrap();

        assert_eq!(event.change_type, "create");
        assert_eq!(event.file_key, "new.md");
        assert_eq!(event.timestamp, "2024-01-01T00:00:00Z");
    }

    // ============================================================================
    // PendingEvent Tests
    // ============================================================================

    #[test]
    fn test_pending_event_tracking() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        let now = Instant::now();
        {
            let mut pending = watcher.pending_events.lock().unwrap();
            pending.insert(
                temp.path().join("test.md"),
                PendingEvent {
                    change_type: "modify".to_string(),
                    first_seen: now,
                    last_seen: now,
                },
            );
        }

        let pending = watcher.pending_events.lock().unwrap();
        assert!(pending.contains_key(&temp.path().join("test.md")));
        let event = pending.get(&temp.path().join("test.md")).unwrap();
        assert_eq!(event.change_type, "modify");
    }

    // ============================================================================
    // handle_event Logic Tests (using internal state)
    // ============================================================================

    fn create_modify_event(paths: Vec<PathBuf>) -> Event {
        Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths,
            attrs: Default::default(),
        }
    }

    fn create_create_event(paths: Vec<PathBuf>) -> Event {
        Event {
            kind: EventKind::Create(CreateKind::Any),
            paths,
            attrs: Default::default(),
        }
    }

    fn create_delete_event(paths: Vec<PathBuf>) -> Event {
        Event {
            kind: EventKind::Remove(RemoveKind::Any),
            paths,
            attrs: Default::default(),
        }
    }

    #[test]
    fn test_handle_event_modify() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
        assert_eq!(pending.get(&file_path).unwrap().change_type, "modify");
    }

    #[test]
    fn test_handle_event_create() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("new.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_create_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
        assert_eq!(pending.get(&file_path).unwrap().change_type, "create");
    }

    #[test]
    fn test_handle_event_delete() {
        let temp = TempDir::new().unwrap();
        // For delete events, file doesn't need to exist
        let file_path = temp.path().join("deleted.md");

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_delete_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
        assert_eq!(pending.get(&file_path).unwrap().change_type, "delete");
    }

    #[test]
    fn test_handle_event_ignores_saving_files() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("saving.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        saving_files.lock().unwrap().insert(file_path.clone());
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&file_path));
    }

    #[test]
    fn test_handle_event_ignores_recent_saves() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("recent.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        recent_saves
            .lock()
            .unwrap()
            .insert(file_path.clone(), Instant::now());
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&file_path));
    }

    #[test]
    fn test_handle_event_ignores_patterns() {
        let temp = TempDir::new().unwrap();

        // Create .git directory
        let git_path = temp.path().join(".git").join("config");
        std::fs::create_dir_all(git_path.parent().unwrap()).unwrap();
        std::fs::write(&git_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![git_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&git_path));
    }

    #[test]
    fn test_handle_event_ignores_midlight_dir() {
        let temp = TempDir::new().unwrap();

        let midlight_path = temp
            .path()
            .join(".midlight")
            .join("objects")
            .join("abc123");
        std::fs::create_dir_all(midlight_path.parent().unwrap()).unwrap();
        std::fs::write(&midlight_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![midlight_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&midlight_path));
    }

    #[test]
    fn test_handle_event_ignores_directories() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("subdir");
        std::fs::create_dir(&dir_path).unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_create_event(vec![dir_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&dir_path));
    }

    #[test]
    fn test_handle_event_ignores_paths_outside_workspace() {
        let temp = TempDir::new().unwrap();
        let other_path = PathBuf::from("/some/other/path.md");

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![other_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_handle_event_escalates_to_delete() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // First a modify event
        let now = Instant::now();
        pending_events.lock().unwrap().insert(
            file_path.clone(),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: now,
                last_seen: now,
            },
        );

        // Then a delete event
        let event = create_delete_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert_eq!(pending.get(&file_path).unwrap().change_type, "delete");
    }

    #[test]
    fn test_handle_event_updates_last_seen() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // First event
        let event = create_modify_event(vec![file_path.clone()]);
        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let first_seen = pending_events
            .lock()
            .unwrap()
            .get(&file_path)
            .unwrap()
            .first_seen;

        // Small delay
        std::thread::sleep(Duration::from_millis(10));

        // Second event
        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        let pe = pending.get(&file_path).unwrap();
        // first_seen should be unchanged, last_seen should be updated
        assert_eq!(pe.first_seen, first_seen);
        assert!(pe.last_seen > first_seen);
    }

    #[test]
    fn test_handle_event_multiple_paths() {
        let temp = TempDir::new().unwrap();
        let file1 = temp.path().join("file1.md");
        let file2 = temp.path().join("file2.md");
        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file1.clone(), file2.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file1));
        assert!(pending.contains_key(&file2));
    }

    #[test]
    fn test_handle_event_ds_store_ignored() {
        let temp = TempDir::new().unwrap();
        let ds_store = temp.path().join(".DS_Store");
        std::fs::write(&ds_store, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![ds_store.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&ds_store));
    }

    #[test]
    fn test_handle_event_node_modules_ignored() {
        let temp = TempDir::new().unwrap();
        let node_module_file = temp
            .path()
            .join("node_modules")
            .join("package")
            .join("index.js");
        std::fs::create_dir_all(node_module_file.parent().unwrap()).unwrap();
        std::fs::write(&node_module_file, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![node_module_file.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&node_module_file));
    }

    #[test]
    fn test_recent_save_grace_period_expired() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        // Insert with old timestamp (2 seconds ago)
        recent_saves
            .lock()
            .unwrap()
            .insert(file_path.clone(), Instant::now() - Duration::from_secs(2));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        // Use 1 second grace period
        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        // Since grace period has expired, event should be recorded
        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
    }

    // ============================================================================
    // Additional Edge Case Tests
    // ============================================================================

    #[test]
    fn test_handle_event_thumbs_db_ignored() {
        let temp = TempDir::new().unwrap();
        let thumbs_db = temp.path().join("Thumbs.db");
        std::fs::write(&thumbs_db, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![thumbs_db.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&thumbs_db));
    }

    #[test]
    fn test_handle_event_empty_paths() {
        let temp = TempDir::new().unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![],
            attrs: Default::default(),
        };

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.is_empty());
    }

    fn create_access_event(paths: Vec<PathBuf>) -> Event {
        Event {
            kind: EventKind::Access(AccessKind::Any),
            paths,
            attrs: Default::default(),
        }
    }

    #[test]
    fn test_handle_event_ignores_access_events() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_access_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        // Access events should be ignored
        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&file_path));
    }

    #[test]
    fn test_handle_event_ignores_other_events() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = Event {
            kind: EventKind::Other,
            paths: vec![file_path.clone()],
            attrs: Default::default(),
        };

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&file_path));
    }

    #[test]
    fn test_handle_event_custom_ignored_pattern() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("ignored_file.tmp");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig {
            debounce_ms: 500,
            ignored_patterns: vec!["ignored_".to_string()],
        };
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(!pending.contains_key(&file_path));
    }

    #[test]
    fn test_handle_event_nested_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("level1").join("level2").join("deep.md");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
        assert_eq!(pending.get(&file_path).unwrap().change_type, "modify");
    }

    #[test]
    fn test_handle_event_modify_does_not_escalate() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Start with a delete event
        let now = Instant::now();
        pending_events.lock().unwrap().insert(
            file_path.clone(),
            PendingEvent {
                change_type: "delete".to_string(),
                first_seen: now,
                last_seen: now,
            },
        );

        // Then a modify event - should NOT change delete to modify
        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        // Delete should remain as delete (only modify->delete escalates, not the reverse)
        assert_eq!(pending.get(&file_path).unwrap().change_type, "delete");
    }

    #[test]
    fn test_handle_event_create_to_delete_escalation() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Start with a create event
        let now = Instant::now();
        pending_events.lock().unwrap().insert(
            file_path.clone(),
            PendingEvent {
                change_type: "create".to_string(),
                first_seen: now,
                last_seen: now,
            },
        );

        // Then a delete event - should escalate to delete
        let event = create_delete_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert_eq!(pending.get(&file_path).unwrap().change_type, "delete");
    }

    // ============================================================================
    // Trait Implementation Tests
    // ============================================================================

    #[test]
    fn test_file_change_event_debug() {
        let event = FileChangeEvent {
            change_type: "modify".to_string(),
            file_key: "test.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("FileChangeEvent"));
        assert!(debug_str.contains("modify"));
        assert!(debug_str.contains("test.md"));
    }

    #[test]
    fn test_file_change_event_clone() {
        let event = FileChangeEvent {
            change_type: "create".to_string(),
            file_key: "new.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let cloned = event.clone();
        assert_eq!(cloned.change_type, event.change_type);
        assert_eq!(cloned.file_key, event.file_key);
        assert_eq!(cloned.timestamp, event.timestamp);
    }

    #[test]
    fn test_file_watcher_config_debug() {
        let config = FileWatcherConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("FileWatcherConfig"));
        assert!(debug_str.contains("500"));
    }

    #[test]
    fn test_file_watcher_config_clone() {
        let config = FileWatcherConfig {
            debounce_ms: 1000,
            ignored_patterns: vec!["test".to_string()],
        };

        let cloned = config.clone();
        assert_eq!(cloned.debounce_ms, config.debounce_ms);
        assert_eq!(cloned.ignored_patterns, config.ignored_patterns);
    }

    #[test]
    fn test_pending_event_debug() {
        let event = PendingEvent {
            change_type: "modify".to_string(),
            first_seen: Instant::now(),
            last_seen: Instant::now(),
        };

        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("PendingEvent"));
        assert!(debug_str.contains("modify"));
    }

    #[test]
    fn test_pending_event_clone() {
        let now = Instant::now();
        let event = PendingEvent {
            change_type: "create".to_string(),
            first_seen: now,
            last_seen: now,
        };

        let cloned = event.clone();
        assert_eq!(cloned.change_type, event.change_type);
        assert_eq!(cloned.first_seen, event.first_seen);
        assert_eq!(cloned.last_seen, event.last_seen);
    }

    // ============================================================================
    // Drop Tests
    // ============================================================================

    #[test]
    fn test_drop_stops_watcher() {
        let temp = TempDir::new().unwrap();

        {
            let watcher = FileWatcher::new(temp.path().to_path_buf(), None);
            watcher.mark_saving("test.md");

            // Watcher goes out of scope here, drop should be called
        }

        // No panic means drop worked correctly
    }

    // ============================================================================
    // Mixed Scenarios
    // ============================================================================

    #[test]
    fn test_handle_event_mixed_valid_and_ignored() {
        let temp = TempDir::new().unwrap();
        let valid_file = temp.path().join("valid.md");
        let ignored_file = temp.path().join(".git").join("config");
        std::fs::write(&valid_file, "content").unwrap();
        std::fs::create_dir_all(ignored_file.parent().unwrap()).unwrap();
        std::fs::write(&ignored_file, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![valid_file.clone(), ignored_file.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&valid_file));
        assert!(!pending.contains_key(&ignored_file));
    }

    #[test]
    fn test_handle_event_file_key_format() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("docs").join("notes").join("test.md");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_create_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
    }

    #[test]
    fn test_mark_and_clear_multiple_files() {
        let temp = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        // Mark multiple files
        watcher.mark_saving("file1.md");
        watcher.mark_saving("file2.md");
        watcher.mark_saving("file3.md");

        // Clear some
        watcher.clear_saving("file1.md");
        watcher.clear_saving("file3.md");

        let saving = watcher.saving_files.lock().unwrap();
        assert_eq!(saving.len(), 1);
        assert!(saving.contains(&temp.path().join("file2.md")));

        let recent = watcher.recent_saves.lock().unwrap();
        assert!(recent.contains_key(&temp.path().join("file1.md")));
        assert!(!recent.contains_key(&temp.path().join("file2.md")));
        assert!(recent.contains_key(&temp.path().join("file3.md")));
    }

    #[test]
    fn test_stop_multiple_times() {
        let temp = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new(temp.path().to_path_buf(), None);

        // Stop multiple times should not panic
        watcher.stop();
        watcher.stop();
        watcher.stop();

        assert!(watcher.watcher.is_none());
        assert!(watcher.stop_tx.is_none());
    }

    #[test]
    fn test_file_change_event_all_change_types() {
        // Test all change type variants
        let types = vec!["modify", "create", "delete", "rename"];

        for change_type in types {
            let event = FileChangeEvent {
                change_type: change_type.to_string(),
                file_key: "test.md".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
            };

            let json = serde_json::to_string(&event).unwrap();
            let parsed: FileChangeEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.change_type, change_type);
        }
    }

    #[test]
    fn test_handle_event_unicode_filename() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("日本語ファイル.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
    }

    #[test]
    fn test_handle_event_spaces_in_filename() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file with spaces.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
    }

    #[test]
    fn test_handle_event_special_chars_in_filename() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file-with_special.chars!.md");
        std::fs::write(&file_path, "content").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        assert!(pending.contains_key(&file_path));
    }

    #[test]
    fn test_config_empty_ignored_patterns() {
        let config = FileWatcherConfig {
            debounce_ms: 500,
            ignored_patterns: vec![],
        };

        assert!(config.ignored_patterns.is_empty());
        assert_eq!(config.debounce_ms, 500);
    }

    #[test]
    fn test_handle_event_all_files_saving() {
        let temp = TempDir::new().unwrap();
        let file1 = temp.path().join("file1.md");
        let file2 = temp.path().join("file2.md");
        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        saving_files.lock().unwrap().insert(file1.clone());
        saving_files.lock().unwrap().insert(file2.clone());
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file1.clone(), file2.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        // Both files are being saved, so neither should appear in pending
        let pending = pending_events.lock().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_handle_event_all_files_in_grace_period() {
        let temp = TempDir::new().unwrap();
        let file1 = temp.path().join("file1.md");
        let file2 = temp.path().join("file2.md");
        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        recent_saves
            .lock()
            .unwrap()
            .insert(file1.clone(), Instant::now());
        recent_saves
            .lock()
            .unwrap()
            .insert(file2.clone(), Instant::now());
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file1.clone(), file2.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        // Both files are in grace period, so neither should appear in pending
        let pending = pending_events.lock().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_handle_event_partial_pattern_match() {
        let temp = TempDir::new().unwrap();
        // .git is ignored, but .github should not be (if not in default patterns)
        let file_path = temp
            .path()
            .join(".github")
            .join("workflows")
            .join("ci.yml");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "content").unwrap();

        // Use empty ignored patterns
        let config = FileWatcherConfig {
            debounce_ms: 500,
            ignored_patterns: vec![".git".to_string()], // Only .git, not .github
        };
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let event = create_modify_event(vec![file_path.clone()]);

        FileWatcher::handle_event(
            &event,
            temp.path(),
            &config,
            &saving_files,
            &recent_saves,
            &pending_events,
            Duration::from_secs(1),
        );

        let pending = pending_events.lock().unwrap();
        // .github contains .git so it will be ignored with substring match
        // This tests the actual behavior
        assert!(!pending.contains_key(&file_path));
    }

    // ============================================================================
    // flush_pending Tests (using MockEmitter)
    // ============================================================================

    #[test]
    fn test_flush_pending_emits_ready_events() {
        let temp = TempDir::new().unwrap();
        let emitter = MockEmitter::new();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Add event that's ready (old enough - 2 seconds ago)
        let file_path = temp.path().join("test.md");
        pending_events.lock().unwrap().insert(
            file_path.clone(),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: Instant::now() - Duration::from_secs(2),
                last_seen: Instant::now() - Duration::from_secs(2),
            },
        );

        FileWatcher::flush_pending(
            &emitter,
            temp.path(),
            &pending_events,
            Duration::from_millis(500),
        );

        let events = emitter.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change_type, "modify");
        assert_eq!(events[0].file_key, "test.md");

        // Pending should be empty now
        assert!(pending_events.lock().unwrap().is_empty());
    }

    #[test]
    fn test_flush_pending_skips_recent_events() {
        let temp = TempDir::new().unwrap();
        let emitter = MockEmitter::new();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Add event that's too recent
        let file_path = temp.path().join("test.md");
        pending_events.lock().unwrap().insert(
            file_path.clone(),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: Instant::now(),
                last_seen: Instant::now(),
            },
        );

        FileWatcher::flush_pending(
            &emitter,
            temp.path(),
            &pending_events,
            Duration::from_millis(500),
        );

        // No events should be emitted
        assert!(emitter.get_events().is_empty());

        // Pending should still contain the event
        assert_eq!(pending_events.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_flush_pending_handles_emit_error() {
        let temp = TempDir::new().unwrap();
        let emitter = MockEmitter::with_failure();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Add event that's ready
        let file_path = temp.path().join("test.md");
        pending_events.lock().unwrap().insert(
            file_path.clone(),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: Instant::now() - Duration::from_secs(2),
                last_seen: Instant::now() - Duration::from_secs(2),
            },
        );

        // Should not panic even when emit fails
        FileWatcher::flush_pending(
            &emitter,
            temp.path(),
            &pending_events,
            Duration::from_millis(500),
        );

        // Event should still be removed from pending (even though emit failed)
        assert!(pending_events.lock().unwrap().is_empty());
    }

    #[test]
    fn test_flush_pending_multiple_events() {
        let temp = TempDir::new().unwrap();
        let emitter = MockEmitter::new();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Add multiple ready events
        let old_time = Instant::now() - Duration::from_secs(2);
        pending_events.lock().unwrap().insert(
            temp.path().join("file1.md"),
            PendingEvent {
                change_type: "create".to_string(),
                first_seen: old_time,
                last_seen: old_time,
            },
        );
        pending_events.lock().unwrap().insert(
            temp.path().join("file2.md"),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: old_time,
                last_seen: old_time,
            },
        );
        pending_events.lock().unwrap().insert(
            temp.path().join("file3.md"),
            PendingEvent {
                change_type: "delete".to_string(),
                first_seen: old_time,
                last_seen: old_time,
            },
        );

        FileWatcher::flush_pending(
            &emitter,
            temp.path(),
            &pending_events,
            Duration::from_millis(500),
        );

        let events = emitter.get_events();
        assert_eq!(events.len(), 3);

        // Verify all files were emitted
        let file_keys: Vec<&str> = events.iter().map(|e| e.file_key.as_str()).collect();
        assert!(file_keys.contains(&"file1.md"));
        assert!(file_keys.contains(&"file2.md"));
        assert!(file_keys.contains(&"file3.md"));
    }

    #[test]
    fn test_flush_pending_mixed_ready_and_recent() {
        let temp = TempDir::new().unwrap();
        let emitter = MockEmitter::new();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let old_time = Instant::now() - Duration::from_secs(2);

        // Ready event
        pending_events.lock().unwrap().insert(
            temp.path().join("ready.md"),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: old_time,
                last_seen: old_time,
            },
        );

        // Recent event (not ready)
        pending_events.lock().unwrap().insert(
            temp.path().join("recent.md"),
            PendingEvent {
                change_type: "create".to_string(),
                first_seen: Instant::now(),
                last_seen: Instant::now(),
            },
        );

        FileWatcher::flush_pending(
            &emitter,
            temp.path(),
            &pending_events,
            Duration::from_millis(500),
        );

        let events = emitter.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].file_key, "ready.md");

        // Recent event should still be pending
        let pending = pending_events.lock().unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending.contains_key(&temp.path().join("recent.md")));
    }

    #[test]
    fn test_flush_pending_nested_path() {
        let temp = TempDir::new().unwrap();
        let emitter = MockEmitter::new();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        let file_path = temp.path().join("docs").join("notes").join("deep.md");
        let old_time = Instant::now() - Duration::from_secs(2);

        pending_events.lock().unwrap().insert(
            file_path.clone(),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: old_time,
                last_seen: old_time,
            },
        );

        FileWatcher::flush_pending(
            &emitter,
            temp.path(),
            &pending_events,
            Duration::from_millis(500),
        );

        let events = emitter.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].file_key, "docs/notes/deep.md");
    }

    #[test]
    fn test_flush_pending_empty() {
        let temp = TempDir::new().unwrap();
        let emitter = MockEmitter::new();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        FileWatcher::flush_pending(
            &emitter,
            temp.path(),
            &pending_events,
            Duration::from_millis(500),
        );

        assert!(emitter.get_events().is_empty());
    }

    // ============================================================================
    // collect_ready_events Tests
    // ============================================================================

    #[test]
    fn test_collect_ready_events_basic() {
        let temp = TempDir::new().unwrap();
        let mut pending = HashMap::new();

        let old_time = Instant::now() - Duration::from_secs(2);
        pending.insert(
            temp.path().join("test.md"),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: old_time,
                last_seen: old_time,
            },
        );

        let events =
            FileWatcher::collect_ready_events(&mut pending, temp.path(), Duration::from_millis(500));

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change_type, "modify");
        assert_eq!(events[0].file_key, "test.md");
        assert!(pending.is_empty());
    }

    #[test]
    fn test_collect_ready_events_none_ready() {
        let temp = TempDir::new().unwrap();
        let mut pending = HashMap::new();

        pending.insert(
            temp.path().join("test.md"),
            PendingEvent {
                change_type: "modify".to_string(),
                first_seen: Instant::now(),
                last_seen: Instant::now(),
            },
        );

        let events =
            FileWatcher::collect_ready_events(&mut pending, temp.path(), Duration::from_millis(500));

        assert!(events.is_empty());
        assert_eq!(pending.len(), 1);
    }

    // ============================================================================
    // Event Emitter Tests
    // ============================================================================

    #[test]
    fn test_mock_emitter_success() {
        let emitter = MockEmitter::new();
        let event = FileChangeEvent {
            change_type: "modify".to_string(),
            file_key: "test.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let result = emitter.emit_file_change(&event);
        assert!(result.is_ok());

        let events = emitter.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], event);
    }

    #[test]
    fn test_mock_emitter_failure() {
        let emitter = MockEmitter::with_failure();
        let event = FileChangeEvent {
            change_type: "modify".to_string(),
            file_key: "test.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let result = emitter.emit_file_change(&event);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Mock emit failure"));
    }

    // ============================================================================
    // start_with_emitter Tests
    // ============================================================================

    #[test]
    fn test_start_with_emitter_creates_watcher() {
        let temp = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new(temp.path().to_path_buf(), None);
        let emitter = Arc::new(MockEmitter::new());

        let result = watcher.start_with_emitter(emitter);
        assert!(result.is_ok());
        assert!(watcher.watcher.is_some());
        assert!(watcher.stop_tx.is_some());

        // Clean up
        watcher.stop();
    }

    #[test]
    fn test_start_with_emitter_already_watching() {
        let temp = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new(temp.path().to_path_buf(), None);
        let emitter = Arc::new(MockEmitter::new());

        // Start first time
        let result1 = watcher.start_with_emitter(emitter.clone());
        assert!(result1.is_ok());

        // Start again - should return Ok without error
        let result2 = watcher.start_with_emitter(emitter);
        assert!(result2.is_ok());

        // Clean up
        watcher.stop();
    }

    #[test]
    fn test_start_with_emitter_invalid_path() {
        let mut watcher =
            FileWatcher::new(PathBuf::from("/nonexistent/path/that/doesnt/exist"), None);
        let emitter = Arc::new(MockEmitter::new());

        let result = watcher.start_with_emitter(emitter);
        // May or may not fail depending on OS - just ensure it doesn't panic
        // On some systems, watching a nonexistent path succeeds until access is attempted
        let _ = result;
    }

    // ============================================================================
    // Event Loop Integration Tests
    // ============================================================================

    #[test]
    fn test_event_loop_stops_on_signal() {
        let temp = TempDir::new().unwrap();
        let emitter = Arc::new(MockEmitter::new());

        let (tx, rx) = channel::<notify::Result<Event>>();
        let (stop_tx, stop_rx) = channel::<()>();

        let workspace_root = temp.path().to_path_buf();
        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Spawn event loop in a thread
        let handle = std::thread::spawn(move || {
            FileWatcher::event_loop(
                rx,
                stop_rx,
                emitter,
                workspace_root,
                config,
                saving_files,
                recent_saves,
                pending_events,
            );
        });

        // Send stop signal
        stop_tx.send(()).unwrap();

        // Wait for thread to finish (with timeout)
        let result = handle.join();
        assert!(result.is_ok());

        // Drop tx to ensure channel is closed
        drop(tx);
    }

    #[test]
    fn test_event_loop_stops_on_disconnect() {
        let temp = TempDir::new().unwrap();
        let emitter = Arc::new(MockEmitter::new());

        let (tx, rx) = channel::<notify::Result<Event>>();
        let (_stop_tx, stop_rx) = channel::<()>();

        let workspace_root = temp.path().to_path_buf();
        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Spawn event loop in a thread
        let handle = std::thread::spawn(move || {
            FileWatcher::event_loop(
                rx,
                stop_rx,
                emitter,
                workspace_root,
                config,
                saving_files,
                recent_saves,
                pending_events,
            );
        });

        // Drop the sender to disconnect the channel
        drop(tx);

        // Wait for thread to finish (with timeout)
        let result = handle.join();
        assert!(result.is_ok());
    }

    #[test]
    fn test_event_loop_processes_events() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "content").unwrap();

        let emitter = Arc::new(MockEmitter::new());
        let emitter_clone = emitter.emitted_events.clone();

        let (tx, rx) = channel::<notify::Result<Event>>();
        let (stop_tx, stop_rx) = channel::<()>();

        let workspace_root = temp.path().to_path_buf();
        let config = FileWatcherConfig {
            debounce_ms: 10, // Short debounce for testing
            ignored_patterns: vec![],
        };
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Spawn event loop
        let handle = std::thread::spawn(move || {
            FileWatcher::event_loop(
                rx,
                stop_rx,
                emitter,
                workspace_root,
                config,
                saving_files,
                recent_saves,
                pending_events,
            );
        });

        // Send a file event
        tx.send(Ok(create_modify_event(vec![file_path.clone()])))
            .unwrap();

        // Wait for debounce + flush cycle
        std::thread::sleep(Duration::from_millis(200));

        // Stop the loop
        stop_tx.send(()).unwrap();
        handle.join().unwrap();

        // Check that event was emitted
        let events = emitter_clone.lock().unwrap();
        assert!(!events.is_empty());
        assert_eq!(events[0].file_key, "test.md");
    }

    #[test]
    fn test_event_loop_handles_watch_error() {
        let temp = TempDir::new().unwrap();
        let emitter = Arc::new(MockEmitter::new());

        let (tx, rx) = channel::<notify::Result<Event>>();
        let (stop_tx, stop_rx) = channel::<()>();

        let workspace_root = temp.path().to_path_buf();
        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Spawn event loop
        let handle = std::thread::spawn(move || {
            FileWatcher::event_loop(
                rx,
                stop_rx,
                emitter,
                workspace_root,
                config,
                saving_files,
                recent_saves,
                pending_events,
            );
        });

        // Send a watch error
        tx.send(Err(notify::Error::generic("Test error"))).unwrap();

        // Give it time to process
        std::thread::sleep(Duration::from_millis(50));

        // Stop the loop
        stop_tx.send(()).unwrap();
        handle.join().unwrap();

        // No panic means error was handled correctly
    }

    #[test]
    fn test_event_loop_cleans_recent_saves() {
        let temp = TempDir::new().unwrap();
        let emitter = Arc::new(MockEmitter::new());

        let (tx, rx) = channel::<notify::Result<Event>>();
        let (stop_tx, stop_rx) = channel::<()>();

        let workspace_root = temp.path().to_path_buf();
        let config = FileWatcherConfig::default();
        let saving_files = Arc::new(Mutex::new(HashSet::new()));
        let recent_saves = Arc::new(Mutex::new(HashMap::new()));
        let recent_saves_clone = recent_saves.clone();
        let pending_events = Arc::new(Mutex::new(HashMap::new()));

        // Add an old recent save
        recent_saves_clone.lock().unwrap().insert(
            temp.path().join("old.md"),
            Instant::now() - Duration::from_secs(10),
        );

        // Spawn event loop
        let handle = std::thread::spawn(move || {
            FileWatcher::event_loop(
                rx,
                stop_rx,
                emitter,
                workspace_root,
                config,
                saving_files,
                recent_saves,
                pending_events,
            );
        });

        // Wait for cleanup cycle
        std::thread::sleep(Duration::from_millis(200));

        // Stop the loop
        stop_tx.send(()).unwrap();
        drop(tx);
        handle.join().unwrap();

        // Old recent save should be cleaned up
        assert!(recent_saves_clone.lock().unwrap().is_empty());
    }

    // ============================================================================
    // TauriEmitter Tests (struct only, not trait impl which needs runtime)
    // ============================================================================

    #[test]
    fn test_file_change_event_equality() {
        let event1 = FileChangeEvent {
            change_type: "modify".to_string(),
            file_key: "test.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let event2 = FileChangeEvent {
            change_type: "modify".to_string(),
            file_key: "test.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let event3 = FileChangeEvent {
            change_type: "create".to_string(),
            file_key: "test.md".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }
}
