# Phase 7: Recovery & Polish - Architecture Document

**Status:** COMPLETE
**Last Updated:** January 8, 2025
**Author:** Architecture Review

---

## Implementation Progress

| Component | Status | Notes |
|-----------|--------|-------|
| Recovery Manager | **COMPLETE** | Hybrid WAL with debouncing, RecoveryDialog UI |
| Toast Notifications | **COMPLETE** | ToastContainer with auto-dismiss, pause on hover |
| Document Search | **COMPLETE** | Enhanced SearchDropdown with scoring, Ctrl+K/Ctrl+P |
| File Watcher | **COMPLETE** | Native events via notify crate, ExternalChangeDialog UI |
| Error Reporting | **COMPLETE** | PII sanitization, opt-in, rate limiting, settings toggle |
| Keyboard Shortcuts | **COMPLETE** | Registry pattern, platform-aware modifiers, settings view |

### Completed Files

**Recovery Manager:**
- `src-tauri/src/services/recovery_manager.rs` - Rust WAL service with xxHash
- `src-tauri/src/commands/recovery.rs` - 8 Tauri IPC commands
- `packages/stores/src/recovery.ts` - Svelte store with debounced writes (2s)
- `apps/desktop/src/lib/recovery.ts` - Recovery client
- `apps/desktop/src/lib/components/RecoveryDialog.svelte` - Recovery UI

**Toast Notifications:**
- `packages/stores/src/toast.ts` - Toast queue management, auto-dismiss
- `apps/desktop/src/lib/components/Toast.svelte` - Individual toast component
- `apps/desktop/src/lib/components/ToastContainer.svelte` - Fixed position stack

**File Watcher:**
- `src-tauri/src/services/file_watcher.rs` - Native events via notify crate
- `src-tauri/src/commands/file_watcher.rs` - Tauri commands (start, stop, mark/clear saving)
- `packages/stores/src/fileWatcher.ts` - Svelte store for external changes
- `apps/desktop/src/lib/fileWatcher.ts` - File watcher client
- `apps/desktop/src/lib/components/ExternalChangeDialog.svelte` - External change UI

**Document Search:**
- Enhanced `SearchDropdown.svelte` with scoring algorithm
- Added Ctrl+P as alternative to Ctrl+K in `SearchBar.svelte`

**Error Reporting:**
- `src-tauri/src/services/error_reporter.rs` - Rust service with PII sanitization, rate limiting
- `src-tauri/src/commands/error_reporter.rs` - Tauri commands (set_enabled, get_status, report)
- `apps/desktop/src/lib/errorReporter.ts` - TypeScript client with convenience functions
- Settings toggle in General tab with privacy disclosure

**Keyboard Shortcuts:**
- `packages/stores/src/shortcuts.ts` - Shortcut registry with platform detection
- App shortcuts registered in `App.svelte` (save, open, close, toggle panels)
- Shortcuts reference tab added to Settings modal

---

## Executive Summary

Phase 7 addresses critical reliability and user experience features: crash recovery, external file change detection, error reporting, notifications, and document search. This document explores implementation approaches, analyzes trade-offs, and provides recommendations for robust, long-term solutions.

---

## Table of Contents

1. [Requirements Analysis](#1-requirements-analysis)
2. [Recovery System](#2-recovery-system)
3. [File Watching](#3-file-watching)
4. [Error Reporting](#4-error-reporting)
5. [Toast Notifications](#5-toast-notifications)
6. [Document Search](#6-document-search)
7. [Keyboard Shortcuts](#7-keyboard-shortcuts)
8. [Implementation Priorities](#8-implementation-priorities)
9. [Risk Assessment](#9-risk-assessment)

---

## 1. Requirements Analysis

### Core Requirements

| Feature | Priority | Complexity | User Impact |
|---------|----------|------------|-------------|
| Crash Recovery | P0 | High | Critical - prevents data loss |
| File Watching | P1 | Medium | High - prevents conflicts |
| Toast Notifications | P1 | Low | High - user feedback |
| Document Search | P1 | Medium | High - productivity |
| Error Reporting | P2 | Medium | Medium - helps debugging |
| Keyboard Shortcuts | P2 | Low | Medium - power users |

### Non-Functional Requirements

- **Reliability:** Recovery must work 100% of the time when crash occurs
- **Performance:** File watching must not degrade editor performance
- **Privacy:** Error reporting must be opt-in with PII sanitization
- **Responsiveness:** Search must feel instant (<100ms for typical workspaces)
- **Battery:** File watching must be efficient on laptops

---

## 2. Recovery System

### Problem Statement

Users lose work when the app crashes or system shuts down unexpectedly. We need a system that:
1. Continuously saves unsaved changes to a recovery location
2. Detects orphaned recovery files on startup
3. Prompts users to restore or discard recovered content
4. Cleans up recovery files after successful save

### Approach A: Write-Ahead Logging (WAL)

**How it works:**
- Write document content to a `.wal` file every N milliseconds
- On crash, WAL files remain (orphaned)
- On startup, scan for WAL files and prompt recovery
- On successful save, delete WAL file

**Pros:**
- Simple to implement and reason about
- Works across all platforms
- No dependencies on OS-specific features
- Battle-tested pattern (SQLite, databases)

**Cons:**
- Disk I/O every N milliseconds
- Potential for stale recovery if user leaves app open for days
- Need to handle concurrent edits to same file

**Implementation details:**
```
.midlight/recovery/
├── {sha256(file_key)}.wal.json
└── {sha256(file_key)}.wal.json
```

Each WAL file contains:
```json
{
  "version": 1,
  "file_key": "notes/ideas.md",
  "content": "{\"type\":\"doc\",...}",
  "timestamp": "2025-01-08T12:34:56Z",
  "workspace_root": "/Users/..."
}
```

### Approach B: Operating System Crash Handlers

**How it works:**
- Register signal handlers (SIGTERM, SIGINT, etc.)
- On crash signal, synchronously write all unsaved content
- Rely on OS to deliver signals before termination

**Pros:**
- Only writes on crash, not continuously
- Less disk I/O during normal operation

**Cons:**
- Not reliable - SIGKILL cannot be caught
- Power loss = no signal
- macOS/Windows have different signal semantics
- Complex edge cases (force quit, kernel panic)

### Approach C: Hybrid - WAL with Intelligent Batching

**How it works:**
- Combine WAL with smart batching:
  - Write immediately after significant changes (>100 chars diff)
  - Write after idle period (2 seconds of no typing)
  - Write on periodic interval (30 seconds) as fallback
- Use content hashing to avoid redundant writes

**Pros:**
- Best of both worlds: reliability + efficiency
- Fewer disk writes than pure WAL
- Still recovers on hard crash

**Cons:**
- More complex state management
- Need to track "significant change" threshold

### Recommendation: Approach C (Hybrid WAL)

**Rationale:**
1. Pure WAL (Approach A) with 500ms interval means ~120 writes/minute for active editing - excessive
2. Signal handlers (Approach B) are fundamentally unreliable
3. Hybrid approach writes only when meaningful, typically 5-10 writes/minute

**Proposed Implementation:**

```rust
pub struct RecoveryManager {
    workspace_root: PathBuf,
    recovery_dir: PathBuf,
    active_files: HashMap<String, RecoveryState>,
}

struct RecoveryState {
    file_key: String,
    last_content_hash: u64,      // xxHash for speed
    last_write_time: Instant,
    pending_content: Option<String>,
    write_scheduled: bool,
}

impl RecoveryManager {
    const IDLE_WRITE_DELAY: Duration = Duration::from_secs(2);
    const MAX_WRITE_INTERVAL: Duration = Duration::from_secs(30);
    const MIN_CHANGE_THRESHOLD: usize = 50; // chars

    pub fn on_content_change(&mut self, file_key: &str, content: &str) {
        let hash = xxhash(content);
        let state = self.active_files.get_mut(file_key);

        // Skip if content unchanged
        if state.last_content_hash == hash {
            return;
        }

        // Immediate write if large change
        let change_size = levenshtein_distance(&state.pending_content, content);
        if change_size > Self::MIN_CHANGE_THRESHOLD {
            self.write_wal(file_key, content);
            return;
        }

        // Schedule idle write
        state.pending_content = Some(content.to_string());
        self.schedule_idle_write(file_key);
    }
}
```

**Write triggers:**
1. **Immediate:** Change > 50 characters (paste, AI insert, etc.)
2. **Idle:** 2 seconds after last keystroke
3. **Periodic:** Every 30 seconds if any pending changes
4. **On blur:** When editor loses focus

---

## 3. File Watching

### Problem Statement

When users edit files externally (VS Code, Finder rename, git operations), the app needs to:
1. Detect the change
2. Distinguish between app-initiated and external changes
3. Prompt user to reload or keep local version
4. Handle edge cases (rapid saves, file deletion, renames)

### Approach A: Polling

**How it works:**
- Periodically check file modification times (mtime)
- Compare against cached mtimes
- Emit events when differences detected

**Pros:**
- Works on all platforms identically
- Simple to implement
- No OS-specific dependencies

**Cons:**
- Inefficient - checks all files even when nothing changed
- Latency - changes detected on next poll (100ms-1s)
- Battery impact on laptops

### Approach B: Native File System Events (notify crate)

**How it works:**
- Use OS-native APIs: FSEvents (macOS), inotify (Linux), ReadDirectoryChangesW (Windows)
- Receive events when files change
- Debounce and filter events

**Pros:**
- Efficient - only active when changes occur
- Low latency - near-instant detection
- Battery friendly

**Cons:**
- Platform differences in event semantics
- Event storms during git operations
- Requires careful debouncing

### Approach C: Hybrid - Events with Polling Fallback

**How it works:**
- Use native events as primary mechanism
- Fall back to polling for edge cases:
  - Network drives (events unreliable)
  - When event buffer overflows
  - After wake from sleep

**Pros:**
- Best reliability across all scenarios
- Efficient in common case
- Handles edge cases gracefully

**Cons:**
- More complex implementation
- Need to detect when fallback is needed

### Recommendation: Approach B (Native Events) with Smart Debouncing

**Rationale:**
1. The `notify` crate handles platform differences well
2. Polling is wasteful for 99% of use cases
3. We can handle edge cases through careful event processing rather than polling

**Critical Design Decisions:**

#### 1. Debouncing Strategy

Events often arrive in bursts (save triggers multiple events). Use a two-phase debounce:

```rust
struct FileWatcher {
    pending_events: HashMap<PathBuf, PendingEvent>,
    debounce_duration: Duration,  // 500ms
}

struct PendingEvent {
    first_seen: Instant,
    last_seen: Instant,
    event_type: EventType,
}

impl FileWatcher {
    fn on_raw_event(&mut self, path: PathBuf, event_type: EventType) {
        let now = Instant::now();

        match self.pending_events.get_mut(&path) {
            Some(pending) => {
                // Update existing pending event
                pending.last_seen = now;
                // Escalate event type if needed (modify -> delete = delete)
                pending.event_type = pending.event_type.merge(event_type);
            }
            None => {
                self.pending_events.insert(path, PendingEvent {
                    first_seen: now,
                    last_seen: now,
                    event_type,
                });
            }
        }

        // Schedule flush after debounce period
        self.schedule_flush();
    }

    fn flush_pending(&mut self) {
        let now = Instant::now();
        let ready: Vec<_> = self.pending_events
            .iter()
            .filter(|(_, e)| now - e.last_seen > self.debounce_duration)
            .map(|(p, e)| (p.clone(), e.clone()))
            .collect();

        for (path, event) in ready {
            self.pending_events.remove(&path);
            self.emit_event(path, event);
        }
    }
}
```

#### 2. Distinguishing App vs External Changes

The app must not show "external change" dialogs for its own saves:

```rust
struct FileWatcher {
    saving_files: HashSet<PathBuf>,
    save_grace_period: Duration,  // 1 second
    recent_saves: HashMap<PathBuf, Instant>,
}

impl FileWatcher {
    pub fn mark_saving(&mut self, path: &Path) {
        self.saving_files.insert(path.to_path_buf());
    }

    pub fn clear_saving(&mut self, path: &Path) {
        self.saving_files.remove(path);
        self.recent_saves.insert(path.to_path_buf(), Instant::now());
    }

    fn is_external_change(&self, path: &Path) -> bool {
        // Currently being saved by app
        if self.saving_files.contains(path) {
            return false;
        }

        // Recently saved by app (within grace period)
        if let Some(save_time) = self.recent_saves.get(path) {
            if Instant::now() - *save_time < self.save_grace_period {
                return false;
            }
        }

        true
    }
}
```

#### 3. Handling Rapid External Changes

Git operations can trigger hundreds of events. Batch them:

```rust
fn on_events_batch(&mut self, events: Vec<Event>) {
    // If > 20 events in 100ms, likely a bulk operation
    if events.len() > 20 {
        // Emit single "workspace changed" event
        // Let UI refresh file tree rather than individual prompts
        self.emit_workspace_refresh();
        return;
    }

    // Process individual events
    for event in events {
        self.process_single_event(event);
    }
}
```

---

## 4. Error Reporting

### Problem Statement

We need telemetry to identify and fix bugs, but must:
1. Respect user privacy (opt-in only)
2. Never transmit PII (file paths, usernames, emails)
3. Provide useful debugging information
4. Not impact app performance

### Approach A: First-Party Collection

**How it works:**
- Send error reports to our own endpoint (midlight.ai/api/error-report)
- Store in our database
- Build custom dashboards

**Pros:**
- Full control over data
- No third-party dependencies
- No additional cost

**Cons:**
- Need to build/maintain infrastructure
- Need to build analysis tools
- Storage costs scale with users

### Approach B: Third-Party Service (Sentry, Bugsnag)

**How it works:**
- Integrate Sentry/Bugsnag SDK
- Events automatically captured and sent
- Use their dashboard for analysis

**Pros:**
- Rich features (stack traces, breadcrumbs, release tracking)
- No infrastructure to maintain
- Sophisticated alerting

**Cons:**
- Cost scales with event volume
- Data stored on third-party servers
- SDK adds bundle size
- Privacy concerns (need to configure carefully)

### Approach C: Hybrid - Local-First with Optional Sync

**How it works:**
- Store errors locally first (.midlight/errors/)
- Batch upload when user opts in
- Allow users to review before sending

**Pros:**
- Maximum privacy - nothing sent without explicit action
- Users can see exactly what's being reported
- Works offline

**Cons:**
- Users may never remember to send
- Delayed feedback loop
- More complex UI

### Recommendation: Approach A (First-Party) with Aggressive Sanitization

**Rationale:**
1. We already have a backend (midlight.ai)
2. Error volume will be manageable initially
3. Full control over privacy is essential for trust
4. Can always add Sentry later if needed

**Sanitization Rules (Non-negotiable):**

```rust
pub fn sanitize_message(message: &str) -> String {
    let mut result = message.to_string();

    // 1. File paths - most common PII leak
    // Unix: /Users/username/... or /home/username/...
    let unix_path = Regex::new(r"/(Users|home)/[^/\s]+").unwrap();
    result = unix_path.replace_all(&result, "/$1/[REDACTED]").to_string();

    // Windows: C:\Users\username\...
    let win_path = Regex::new(r"[A-Z]:\\Users\\[^\\\s]+").unwrap();
    result = win_path.replace_all(&result, "C:\\Users\\[REDACTED]").to_string();

    // 2. Email addresses
    let email = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    result = email.replace_all(&result, "[EMAIL]").to_string();

    // 3. UUIDs (might identify users)
    let uuid = Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap();
    result = uuid.replace_all(&result, "[UUID]").to_string();

    // 4. IP addresses
    let ip = Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
    result = ip.replace_all(&result, "[IP]").to_string();

    // 5. Truncate to prevent accidental data exfiltration
    if result.len() > 1000 {
        result = format!("{}... [truncated]", &result[..1000]);
    }

    result
}
```

**Session ID Design:**

Session IDs should be:
- Random (UUID v4)
- Regenerated on each app launch
- Never persisted to disk
- Never correlated with user accounts

```rust
pub struct ErrorReporter {
    session_id: String,  // Generated once at startup
    enabled: bool,

    // Rate limiting to prevent spam
    reports_this_session: AtomicU32,
    max_reports_per_session: u32,  // 50
}

impl ErrorReporter {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            enabled: false,  // Opt-in, default off
            reports_this_session: AtomicU32::new(0),
            max_reports_per_session: 50,
        }
    }
}
```

---

## 5. Toast Notifications

### Problem Statement

Users need non-blocking feedback for:
- Save success/failure
- Import/export progress
- Error messages
- Informational alerts

### Approach A: Native OS Notifications

**How it works:**
- Use Tauri's notification API
- Notifications appear in OS notification center

**Pros:**
- Native look and feel
- Persists in notification center
- Works when app is backgrounded

**Cons:**
- Requires OS permission
- Too heavyweight for in-app feedback
- Users may disable
- Not suitable for transient feedback

### Approach B: In-App Toast Component

**How it works:**
- Custom Svelte component
- Positioned fixed in viewport
- Auto-dismiss after timeout

**Pros:**
- Full control over styling
- No permissions needed
- Can include custom actions
- Immediate, contextual feedback

**Cons:**
- Need to implement from scratch
- Hidden when app not focused

### Approach C: Hybrid - In-App Default, Native for Critical

**How it works:**
- Use in-app toasts for normal feedback
- Use native notifications for critical alerts when app backgrounded

**Pros:**
- Best UX for each scenario
- Critical alerts not missed

**Cons:**
- Two systems to maintain

### Recommendation: Approach B (In-App) with Approach C for Errors

**Rationale:**
1. Most toasts are transient (save succeeded, copied to clipboard)
2. Native notifications are overkill and annoying for these
3. Reserve native notifications for truly critical events (sync conflicts, errors when backgrounded)

**Toast Design:**

```typescript
interface Toast {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  message: string;
  duration: number;      // 0 = persistent until dismissed
  action?: {
    label: string;
    onClick: () => void;
  };
  dismissible: boolean;  // Show X button
}

// Sensible defaults by type
const DEFAULTS = {
  success: { duration: 3000, dismissible: false },
  info: { duration: 5000, dismissible: true },
  warning: { duration: 8000, dismissible: true },
  error: { duration: 0, dismissible: true },  // Persistent
};
```

**Animation Approach:**

Use CSS transforms for performance (GPU accelerated):

```css
.toast {
  transform: translateX(calc(100% + 16px));
  opacity: 0;
  transition: transform 200ms ease-out, opacity 200ms ease-out;
}

.toast.visible {
  transform: translateX(0);
  opacity: 1;
}

.toast.exiting {
  transform: translateX(calc(100% + 16px));
  opacity: 0;
}
```

**Stacking Behavior:**

- New toasts appear at bottom of stack
- Stack grows upward
- Maximum 5 visible (older ones collapse with "+N more")
- Click collapsed indicator to expand all

---

## 6. Document Search

### Problem Statement

Users need to quickly find documents by name. Requirements:
1. Instant results as user types
2. Fuzzy matching for typos
3. Keyboard navigation
4. Show file location context

### Approach A: Simple Substring Match

**How it works:**
- Filter files where name includes query (case-insensitive)
- Show first N matches

**Pros:**
- Trivial to implement
- Very fast
- Predictable results

**Cons:**
- No fuzzy matching ("mdlight" won't find "midlight")
- No ranking by relevance

### Approach B: Fuzzy Search (fzf-style)

**How it works:**
- Use algorithm like fzf or fzy
- Score matches by character positions
- Rank by score

**Pros:**
- Handles typos and abbreviations
- Intuitive for power users
- Better UX

**Cons:**
- More complex scoring algorithm
- May return unexpected results

### Approach C: Full-Text Search (content + names)

**How it works:**
- Index document content
- Search both filenames and content
- Use search engine (MeiliSearch, tantivy)

**Pros:**
- Find documents by content, not just name
- Very powerful

**Cons:**
- Significant implementation complexity
- Need to maintain index
- Memory/storage overhead
- Overkill for Phase 7

### Recommendation: Approach A for Phase 7, Plan for Approach C Later

**Rationale:**
1. Substring matching covers 90% of use cases
2. Users typically remember file names
3. Full-text search is a larger feature (Phase 9+)
4. Can add fuzzy matching incrementally

**Implementation:**

```typescript
interface SearchResult {
  file: FileNode;
  matchIndices: number[];  // For highlighting
  score: number;
}

function search(query: string, files: FileNode[]): SearchResult[] {
  const normalizedQuery = query.toLowerCase();

  return files
    .map(file => {
      const name = file.name.toLowerCase();
      const index = name.indexOf(normalizedQuery);

      if (index === -1) return null;

      // Score: prefer matches at start, then word boundaries
      let score = 100 - index;  // Earlier = better
      if (index === 0) score += 50;  // Starts with query
      if (name[index - 1] === '-' || name[index - 1] === '_') {
        score += 25;  // Word boundary
      }

      return {
        file,
        matchIndices: Array.from(
          { length: query.length },
          (_, i) => index + i
        ),
        score,
      };
    })
    .filter(Boolean)
    .sort((a, b) => b.score - a.score)
    .slice(0, 15);
}
```

**Keyboard Navigation:**

| Key | Action |
|-----|--------|
| ↓ / Ctrl+N | Next result |
| ↑ / Ctrl+P | Previous result |
| Enter | Open selected |
| Escape | Close search |
| Ctrl+Enter | Open in new tab |

---

## 7. Keyboard Shortcuts

### Problem Statement

Power users expect standard keyboard shortcuts. We need:
1. Platform-appropriate modifiers (Cmd on Mac, Ctrl on Windows)
2. No conflicts with browser/OS shortcuts
3. Customizable (future)

### Approach A: Window Event Listeners

**How it works:**
- Add keydown listeners to window
- Check modifier keys and dispatch actions

**Pros:**
- Simple, works everywhere
- Full control
- No dependencies

**Cons:**
- Need to handle focus carefully
- Can conflict with input fields
- No global shortcuts (when app unfocused)

### Approach B: Tauri GlobalShortcut

**How it works:**
- Register shortcuts with Tauri's GlobalShortcut API
- Works even when app is not focused

**Pros:**
- True global shortcuts
- OS-native handling

**Cons:**
- Can conflict with other apps
- Requires permission on some platforms
- Overkill for most shortcuts

### Approach C: Tiptap Extensions for Editor Shortcuts

**How it works:**
- Define editor shortcuts in Tiptap extensions
- Use window listeners for non-editor shortcuts

**Pros:**
- Proper separation of concerns
- Editor shortcuts handled by editor
- Non-editor shortcuts handled by app

**Cons:**
- Two systems (but logically separated)

### Recommendation: Approach C

**Rationale:**
1. Tiptap already handles editor shortcuts (bold, italic, etc.)
2. Window listeners for app-level shortcuts (search, save, etc.)
3. Avoid GlobalShortcut complexity unless specifically needed

**Shortcut Registry:**

```typescript
interface Shortcut {
  id: string;
  keys: string;                    // "mod+k", "mod+shift+p"
  description: string;
  action: () => void;
  when?: () => boolean;            // Only active when condition true
  preventDefault?: boolean;        // Default true
}

const shortcuts: Shortcut[] = [
  {
    id: 'search',
    keys: 'mod+k',
    description: 'Open document search',
    action: () => searchStore.open(),
  },
  {
    id: 'save',
    keys: 'mod+s',
    description: 'Save document',
    action: () => fileSystem.save(),
    when: () => $activeFile !== null,
  },
  {
    id: 'export-pdf',
    keys: 'mod+shift+p',
    description: 'Export to PDF',
    action: () => exportToPdf(),
    when: () => $activeFile !== null,
  },
];

// "mod" is automatically translated to Cmd (Mac) or Ctrl (Windows/Linux)
```

---

## 8. Implementation Priorities

Based on user impact and technical dependencies:

### Phase 7A (Critical Path) - COMPLETE
1. **Recovery Manager** - Prevents data loss (highest priority) ✅
2. **Toast Notifications** - Required for user feedback on all operations ✅

### Phase 7B (High Value) - COMPLETE
3. **Document Search** - High-frequency user action ✅
4. **File Watcher** - Moved up from 7C due to importance ✅

### Phase 7C (Polish) - COMPLETE
5. **Keyboard Shortcuts** - Registry pattern with customization support ✅
6. **Error Reporting** - Opt-in with PII sanitization ✅

### Estimated vs Actual Effort

| Component | Est. Rust | Est. Svelte | Actual Rust | Actual Svelte | Status |
|-----------|-----------|-------------|-------------|---------------|--------|
| Recovery Manager | 300 LOC | 100 LOC | ~300 LOC | ~530 LOC | ✅ Done |
| File Watcher | 400 LOC | 150 LOC | ~380 LOC | ~450 LOC | ✅ Done |
| Toast System | 0 LOC | 200 LOC | 0 LOC | ~350 LOC | ✅ Done |
| Document Search | 0 LOC | 300 LOC | 0 LOC | ~100 LOC | ✅ Done |
| Error Reporting | 200 LOC | 50 LOC | ~200 LOC | ~100 LOC | ✅ Done |
| Keyboard Shortcuts | 0 LOC | 150 LOC | 0 LOC | ~320 LOC | ✅ Done |
| **Total** | **900 LOC** | **950 LOC** | **~880 LOC** | **~1850 LOC** | ✅ All Complete |

*Note: Svelte actuals higher than estimates due to comprehensive UI components with full accessibility support, animations, and edge case handling.*

---

## 9. Risk Assessment

### High Risk

| Risk | Mitigation |
|------|------------|
| Recovery data corruption | Use atomic writes, checksums, version field |
| File watcher event storms | Aggressive debouncing, bulk operation detection |
| PII in error reports | Multiple sanitization passes, truncation |

### Medium Risk

| Risk | Mitigation |
|------|------------|
| Recovery file conflicts (multi-window) | Lock files, or single-window enforcement |
| Toast spam | Rate limiting, deduplication |
| Shortcut conflicts | Avoid browser defaults, allow customization |

### Low Risk

| Risk | Mitigation |
|------|------------|
| Search performance | Limit results, debounce input |
| Error endpoint unavailable | Fire-and-forget, no retry |

---

## Appendix A: File Watcher Edge Cases

### Case 1: Git Checkout

**Scenario:** User runs `git checkout .` which modifies many files.

**Problem:** Event storm, multiple dialogs.

**Solution:** Detect bulk operation (>20 events in 100ms), show single "Files changed externally. Reload all?" prompt.

### Case 2: File Rename

**Scenario:** User renames file in Finder.

**Problem:** May appear as delete + create, not rename.

**Solution:** Correlate delete/create events by content hash within short window.

### Case 3: Network Drive

**Scenario:** Workspace on network drive, another user edits.

**Problem:** FSEvents/inotify unreliable for network paths.

**Solution:** Detect network path, warn user that external changes may not be detected.

### Case 4: Sleep/Wake

**Scenario:** Laptop sleeps, wakes, files were modified.

**Problem:** May miss events during sleep.

**Solution:** On wake, check mtimes of all open files.

---

## Appendix B: Recovery File Format

```json
{
  "version": 2,
  "file_key": "notes/ideas.md",
  "workspace_root": "/Users/[REDACTED]/Documents/my-workspace",
  "content": {
    "type": "doc",
    "content": [...]
  },
  "content_hash": "abc123...",
  "created_at": "2025-01-08T12:34:56.789Z",
  "updated_at": "2025-01-08T12:35:12.345Z",
  "app_version": "1.2.3",
  "write_count": 15
}
```

**Fields:**
- `version`: Schema version for future migrations
- `file_key`: Relative path from workspace root
- `workspace_root`: Helps identify orphaned files from deleted workspaces
- `content`: Full Tiptap document JSON
- `content_hash`: xxHash for quick comparison
- `created_at`: When WAL started
- `updated_at`: Last write time
- `app_version`: For debugging
- `write_count`: Number of times updated (debugging)

---

## Appendix C: Error Report Schema

```json
{
  "schema_version": 1,
  "category": "import",
  "error_type": "yaml_parse",
  "message": "Invalid YAML: unexpected end of stream",
  "sanitized": true,
  "app_version": "1.2.3",
  "platform": "macos",
  "arch": "arm64",
  "os_version": "14.2.1",
  "context": {
    "file_type": "obsidian",
    "file_count": 150
  },
  "timestamp": "2025-01-08T12:34:56Z",
  "session_id": "a1b2c3d4-..."
}
```

**Forbidden fields (never collect):**
- User ID / email / name
- File names / paths (except sanitized type)
- Document content
- IP address (not logged by endpoint)
- Any form of persistent identifier

---

## Conclusion

Phase 7 is primarily about **reliability** and **polish**. The recommendations prioritize:

1. **Data safety** - Recovery system prevents lost work
2. **Privacy** - Error reporting is opt-in with aggressive sanitization
3. **Performance** - Intelligent batching for WAL, debouncing for file watcher
4. **Simplicity** - Start with substring search, add fuzzy later
5. **User experience** - Non-blocking toasts, keyboard shortcuts

The hybrid approaches (WAL with batching, in-app toasts with native fallback) provide the best balance of reliability and efficiency.
