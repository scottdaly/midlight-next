# Midlight Migration Plan: Electron → Tauri + Web

**Goal:** Achieve full feature parity with the existing Electron app while supporting both desktop (Tauri) and web (midlight.ai/editor) platforms.

**Current Status:** Phase 7 (Recovery & Polish) ✅ COMPLETE. All code implemented and compiling.

**Latest Session (January 2025):** Completed Phase 7 Recovery & Polish implementation:
- Recovery Manager: Hybrid WAL with 2-second debouncing, xxHash for deduplication, RecoveryDialog UI
- File Watcher: Native events via `notify` crate, debouncing, ExternalChangeDialog UI
- Toast Notifications: ToastContainer with auto-dismiss, pause on hover, max 5 visible with collapse
- Document Search: Enhanced SearchDropdown with scoring algorithm, Ctrl+K / Ctrl+P shortcuts
- Error Reporting: Opt-in, PII sanitization (paths, emails, UUIDs, IPs), rate limiting (50/session)
- Keyboard Shortcuts: Registry pattern, platform-aware modifiers (Cmd/Ctrl), settings reference

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Architectural Improvements](#architectural-improvements)
3. [Current Progress](#current-progress)
4. [Feature Inventory](#feature-inventory)
5. [Migration Phases](#migration-phases)
6. [Detailed Task Breakdown](#detailed-task-breakdown)
7. [Testing Strategy](#testing-strategy)
8. [Risk Mitigation](#risk-mitigation)

---

## Architecture Overview

### Monorepo Structure

```
midlight-next/
├── packages/
│   ├── core/           # Shared business logic (browser-compatible)
│   │   ├── types/      # Document, Checkpoint, Workspace types
│   │   ├── serialization/  # Tiptap ↔ Markdown + Sidecar
│   │   └── utils/      # Helpers, ID generation, hashing
│   │
│   ├── ui/             # Shared Svelte components
│   │   ├── components/
│   │   │   ├── Editor/     # Tiptap editor + extensions
│   │   │   ├── Sidebar/    # File tree
│   │   │   ├── Chat/       # AI chat panel
│   │   │   ├── Versions/   # Version history
│   │   │   ├── Toolbar/    # Editor toolbar
│   │   │   └── common/     # Buttons, Dialogs, Dropdowns
│   │   └── extensions/     # Custom Tiptap extensions
│   │
│   └── stores/         # Svelte stores
│       ├── fileSystem.ts
│       ├── ai.ts
│       ├── versions.ts
│       ├── auth.ts
│       ├── settings.ts
│       └── agent.ts
│
├── apps/
│   ├── desktop/        # Tauri desktop app
│   │   ├── src-tauri/  # Rust backend
│   │   │   ├── commands/   # IPC handlers
│   │   │   └── services/   # Core services
│   │   └── src/        # Svelte frontend
│   │
│   └── web/            # SvelteKit web app
│       └── src/
│           ├── routes/editor/
│           └── lib/storage/    # OPFS + IndexedDB adapters
│
└── server/             # Backend additions (existing midlight-site)
    └── routes/
        ├── sync.js     # Document sync
        └── documents.js
```

### Platform Abstraction

```
┌─────────────────────────────────────────────────────────────┐
│                    Shared UI Components                      │
│                    (@midlight/ui)                            │
├─────────────────────────────────────────────────────────────┤
│                    Svelte Stores                             │
│                    (@midlight/stores)                        │
├─────────────────────────────────────────────────────────────┤
│                    StorageAdapter Interface                  │
│                    (@midlight/core)                          │
├──────────────────────────┬──────────────────────────────────┤
│    TauriStorageAdapter   │      WebStorageAdapter            │
│    (Rust via IPC)        │      (OPFS + IndexedDB)           │
└──────────────────────────┴──────────────────────────────────┘
```

---

## Architectural Improvements

The migration is an opportunity to address technical debt and leverage the new stack's strengths. These improvements should be incorporated during implementation.

### Summary of Recommendations

| Area | Current (Electron) | Recommended (Tauri/Web) | Impact |
|------|---------|-------------|--------|
| IPC Types | Manual duplication | Code generation via `tauri-specta` | High - eliminates type drift |
| Document Format | Two files (.md + .sidecar.json) | YAML front matter or hidden metadata | Medium - cleaner UX |
| File Watching | Polling with debounce | Push events via Rust channels | Medium - better reactivity |
| AI Context | Ad-hoc string concatenation | Structured protocol with token budgets | High - better AI performance |
| Agent Tools | Scattered definitions | JSON Schema with validation | Medium - runtime safety |
| UI Updates | Blocking operations | Optimistic updates with rollback | High - better UX |
| File I/O | Sequential processing | Parallel with `futures::join_all` | High - performance |
| Reactivity | Full component re-renders | Svelte 5 runes for fine-grained | Medium - performance |
| Error Handling | Uncaught exceptions | Error boundaries + structured errors | Medium - stability |
| Checkpoints | Full content copies | Delta compression | High - 80-90% storage reduction |

---

### 1. Type-Safe IPC with Code Generation

**Problem:** Electron IPC requires manual type definitions on both sides, leading to drift and runtime errors.

**Solution:** Use `tauri-specta` to generate TypeScript types from Rust commands automatically.

```rust
// Rust side - single source of truth
#[tauri::command]
#[specta::specta]
pub async fn workspace_load_document(
    workspace_root: String,
    file_path: String,
) -> Result<LoadedDocument, String> { ... }
```

```typescript
// Generated TypeScript - always in sync
import { commands } from './bindings';
const doc = await commands.workspaceLoadDocument(root, path);
// ^^ Fully typed, no manual definitions needed
```

**Implementation:**
- [ ] Add `tauri-specta` to Cargo.toml
- [ ] Annotate all commands with `#[specta::specta]`
- [ ] Add build script to generate TypeScript bindings
- [ ] Update frontend to use generated types

---

### 2. Unified Document Format

**Problem:** Two files per document (`.md` + `.sidecar.json`) clutters the file system and is fragile.

**Solution:** Use YAML front matter for metadata in a single file.

```markdown
---
midlight:
  version: 1
  blocks:
    p-abc123: { align: "center" }
  spans:
    s-def456: { color: "#ff0000", font: "serif" }
  images:
    img-xyz789: { width: 400 }
---

# My Document

Regular markdown content here...

![Image](midlight://img-xyz789)
```

**Benefits:**
- Single file for simple documents
- Cleaner workspace structure
- Git-friendly (single file to track)
- Compatible with other Markdown tools (metadata is standard YAML)

**Alternative:** Hidden metadata directory
```
workspace/
├── documents/
│   └── notes.md          # Clean markdown only
└── .midlight/
    └── metadata/
        └── notes.json    # All formatting metadata
```

**Implementation:**
- [ ] Update DocumentSerializer to output YAML front matter
- [ ] Update DocumentDeserializer to parse YAML front matter
- [ ] Add migration tool for existing sidecar files
- [ ] Update file operations to handle single-file format

---

### 3. Reactive File System with Rust Channels

**Problem:** File watcher events are polled/debounced awkwardly with manual state tracking.

**Solution:** Use Tauri's event system with Rust channels for real-time push events.

```rust
// Rust - push events directly to frontend
use tauri::Emitter;
use notify::{Watcher, RecursiveMode, Event};

pub fn start_file_watcher(app: AppHandle, root: PathBuf) -> Result<()> {
    let app_clone = app.clone();

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
        if let Ok(event) = res {
            let _ = app_clone.emit("fs:change", FileChangeEvent {
                kind: event.kind.into(),
                paths: event.paths,
                timestamp: chrono::Utc::now(),
            });
        }
    })?;

    watcher.watch(&root, RecursiveMode::Recursive)?;
    Ok(())
}
```

```typescript
// Svelte - reactive subscription
import { listen } from '@tauri-apps/api/event';
import { onMount, onDestroy } from 'svelte';

let unsubscribe: () => void;

onMount(async () => {
    unsubscribe = await listen<FileChangeEvent>('fs:change', (event) => {
        fileSystem.handleExternalChange(event.payload);
    });
});

onDestroy(() => unsubscribe?.());
```

**Implementation:**
- [ ] Create FileWatcher service in Rust with event emission
- [ ] Add `fs:change` event type definitions
- [ ] Create Svelte hook for file change subscriptions
- [ ] Integrate with fileSystem store

---

### 4. Structured AI Context Protocol

**Problem:** AI context is assembled ad-hoc with string concatenation, leading to unpredictable token usage and poor context prioritization.

**Solution:** Define a structured context protocol with explicit sections and token budgets.

```typescript
// packages/core/src/ai/context.ts

interface AIContextSection {
    content: string;
    maxTokens: number;
    priority: number;  // Lower = more important
}

interface AIContext {
    sections: {
        systemPrompt: AIContextSection;
        currentDocument: AIContextSection;
        selectedText: AIContextSection;
        referencedFiles: AIContextSection;
        conversationHistory: AIContextSection;
    };
    totalMaxTokens: number;
}

function buildContext(context: AIContext): string {
    // Sort by priority, truncate from lowest priority first
    const sections = Object.values(context.sections)
        .sort((a, b) => a.priority - b.priority);

    let totalTokens = 0;
    const result: string[] = [];

    for (const section of sections) {
        const tokens = estimateTokens(section.content);
        if (totalTokens + tokens <= context.totalMaxTokens) {
            result.push(section.content);
            totalTokens += tokens;
        } else {
            // Truncate this section to fit remaining budget
            const remaining = context.totalMaxTokens - totalTokens;
            result.push(truncateToTokens(section.content, remaining));
            break;
        }
    }

    return result.join('\n\n');
}
```

**Implementation:**
- [ ] Create AIContext types in @midlight/core
- [ ] Implement token estimation utility
- [ ] Build context builder with priority-based truncation
- [ ] Update AI store to use structured context

---

### 5. Agent Tool Schema with Runtime Validation

**Problem:** Tool definitions are scattered across files, validation is manual and error-prone.

**Solution:** Centralize tool definitions with JSON Schema and runtime validation.

```typescript
// packages/core/src/agent/tools.ts
import Ajv from 'ajv';

export const agentTools = {
    edit_document: {
        name: 'edit_document',
        description: 'Edit content in a document by replacing, appending, or prepending text',
        parameters: {
            type: 'object',
            properties: {
                path: {
                    type: 'string',
                    description: 'Relative file path from workspace root'
                },
                operation: {
                    enum: ['replace', 'append', 'prepend'],
                    description: 'Type of edit operation'
                },
                content: {
                    type: 'string',
                    description: 'New content to insert'
                },
                search: {
                    type: 'string',
                    description: 'Text to find and replace (required for replace operation)'
                },
            },
            required: ['path', 'operation', 'content'],
            if: { properties: { operation: { const: 'replace' } } },
            then: { required: ['path', 'operation', 'content', 'search'] },
        },
        dangerous: false,
        requiresConfirmation: false,
    },
    delete_document: {
        name: 'delete_document',
        description: 'Delete a document (moves to trash)',
        parameters: {
            type: 'object',
            properties: {
                path: { type: 'string', description: 'File path to delete' },
            },
            required: ['path'],
        },
        dangerous: true,
        requiresConfirmation: true,
    },
    // ... other tools
} as const;

// Runtime validation
const ajv = new Ajv();
const validators = Object.fromEntries(
    Object.entries(agentTools).map(([name, tool]) => [
        name,
        ajv.compile(tool.parameters)
    ])
);

export function validateToolCall(name: string, args: unknown): { valid: boolean; errors?: string[] } {
    const validate = validators[name];
    if (!validate) return { valid: false, errors: [`Unknown tool: ${name}`] };

    const valid = validate(args);
    return {
        valid,
        errors: valid ? undefined : validate.errors?.map(e => e.message ?? 'Unknown error'),
    };
}
```

**Implementation:**
- [ ] Create centralized tool definitions in @midlight/core
- [ ] Add ajv for JSON Schema validation
- [ ] Create validateToolCall utility
- [ ] Update agent executor to validate before execution

---

### 6. Optimistic UI with Automatic Rollback

**Problem:** File operations block the UI while waiting for Rust backend response.

**Solution:** Implement optimistic updates with automatic rollback on failure.

```typescript
// packages/stores/src/fileSystem.ts
import { get } from 'svelte/store';

async function renameFile(oldPath: string, newPath: string) {
    const store = get(fileSystem);

    // Capture state for potential rollback
    const previousFiles = [...store.files];
    const previousActiveFile = store.activeFile;

    // Optimistic update - immediate UI response
    fileSystem.update(s => ({
        ...s,
        files: s.files.map(f =>
            f.path === oldPath ? { ...f, path: newPath, name: newPath.split('/').pop()! } : f
        ),
        activeFile: s.activeFile?.path === oldPath
            ? { ...s.activeFile, path: newPath }
            : s.activeFile,
    }));

    try {
        // Actual operation
        await invoke('fs_rename', { oldPath, newPath });
    } catch (error) {
        // Rollback on failure
        fileSystem.update(s => ({
            ...s,
            files: previousFiles,
            activeFile: previousActiveFile,
        }));

        toast.error(`Failed to rename: ${error instanceof Error ? error.message : 'Unknown error'}`);
        throw error;
    }
}

// Higher-order function for optimistic operations
function optimistic<T extends (...args: any[]) => Promise<void>>(
    operation: T,
    getSnapshot: () => Partial<FileSystemState>,
    applyOptimistic: (...args: Parameters<T>) => void,
): T {
    return (async (...args: Parameters<T>) => {
        const snapshot = getSnapshot();
        applyOptimistic(...args);

        try {
            await operation(...args);
        } catch (error) {
            fileSystem.update(s => ({ ...s, ...snapshot }));
            throw error;
        }
    }) as T;
}
```

**Implementation:**
- [ ] Create optimistic wrapper utility
- [ ] Apply to file operations (rename, delete, create, move)
- [ ] Add toast notifications for failures
- [ ] Consider undo stack for complex operations

---

### 7. Parallel File Operations in Rust

**Problem:** Electron processes files sequentially, slow for large directories.

**Solution:** Leverage Rust's async capabilities with `futures::join_all`.

```rust
use futures::future::join_all;
use tokio::fs;

pub async fn load_directory_recursive(root: &Path) -> Result<Vec<FileEntry>> {
    let entries = fs::read_dir(root).await?;
    let mut entry_vec = Vec::new();

    let mut stream = tokio_stream::wrappers::ReadDirStream::new(entries);
    while let Some(entry) = stream.next().await {
        if let Ok(e) = entry {
            entry_vec.push(e);
        }
    }

    // Process all entries in parallel
    let futures: Vec<_> = entry_vec.iter().map(|entry| async {
        let path = entry.path();
        let metadata = fs::metadata(&path).await?;

        let children = if metadata.is_dir() {
            // Recursively load subdirectories in parallel
            Some(Box::pin(load_directory_recursive(&path)).await?)
        } else {
            None
        };

        Ok::<_, std::io::Error>(FileEntry {
            name: entry.file_name().to_string_lossy().into(),
            path: path.to_string_lossy().into(),
            is_directory: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified()?.into(),
            children,
        })
    }).collect();

    let results: Vec<Result<FileEntry, _>> = join_all(futures).await;
    results.into_iter().collect()
}

// Batch file operations
pub async fn copy_files(sources: Vec<PathBuf>, dest: &Path) -> Result<Vec<CopyResult>> {
    let futures: Vec<_> = sources.into_iter().map(|source| {
        let dest = dest.to_owned();
        async move {
            let file_name = source.file_name().unwrap();
            let dest_path = dest.join(file_name);

            match fs::copy(&source, &dest_path).await {
                Ok(bytes) => CopyResult::Success { source, dest: dest_path, bytes },
                Err(e) => CopyResult::Error { source, error: e.to_string() },
            }
        }
    }).collect();

    Ok(join_all(futures).await)
}
```

**Implementation:**
- [ ] Refactor `load_directory` to use parallel async
- [ ] Add parallel batch operations for copy/move
- [ ] Implement progress reporting for large operations
- [ ] Add cancellation support via `tokio::select!`

---

### 8. Svelte 5 Runes for Fine-Grained Reactivity

**Problem:** Zustand re-renders entire components when any store value changes.

**Solution:** Use Svelte 5 runes for surgical, fine-grained updates.

```svelte
<script lang="ts">
    import { fileSystem } from '@midlight/stores';

    // $derived only re-runs when specific dependencies change
    let activeFile = $derived($fileSystem.activeFile);
    let isDirty = $derived($fileSystem.isDirty);

    // Computed values are cached and only recalculate when inputs change
    let wordCount = $derived.by(() => {
        if (!activeFile?.content) return 0;
        return activeFile.content.split(/\s+/).filter(Boolean).length;
    });

    let characterCount = $derived.by(() => {
        return activeFile?.content?.length ?? 0;
    });

    // $effect for side effects - only runs when dependencies change
    $effect(() => {
        if (isDirty) {
            document.title = `* ${activeFile?.name ?? 'Untitled'} - Midlight`;
        } else {
            document.title = `${activeFile?.name ?? 'Untitled'} - Midlight`;
        }
    });
</script>

<!-- Only this span re-renders when wordCount changes -->
<span class="text-muted-foreground text-sm">
    {wordCount} words, {characterCount} characters
</span>
```

**Implementation:**
- [ ] Convert stores to use Svelte 5 runes syntax
- [ ] Update components to use `$derived` for computed values
- [ ] Replace `$:` reactive statements with `$effect`
- [ ] Profile and optimize hot paths

---

### 9. Error Boundary Pattern for Svelte

**Problem:** React has ErrorBoundary, Svelte needs a custom solution.

**Solution:** Create reusable error boundary components.

```svelte
<!-- packages/ui/src/components/common/ErrorBoundary.svelte -->
<script lang="ts">
    import { onMount } from 'svelte';

    interface Props {
        fallback?: import('svelte').Snippet<[Error, () => void]>;
        onError?: (error: Error) => void;
        children: import('svelte').Snippet;
    }

    let { fallback, onError, children }: Props = $props();
    let error = $state<Error | null>(null);

    function reset() {
        error = null;
    }

    onMount(() => {
        const handleError = (event: ErrorEvent) => {
            error = event.error;
            onError?.(event.error);
            event.preventDefault();
        };

        const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
            error = event.reason instanceof Error ? event.reason : new Error(String(event.reason));
            onError?.(error);
            event.preventDefault();
        };

        window.addEventListener('error', handleError);
        window.addEventListener('unhandledrejection', handleUnhandledRejection);

        return () => {
            window.removeEventListener('error', handleError);
            window.removeEventListener('unhandledrejection', handleUnhandledRejection);
        };
    });
</script>

{#if error}
    {#if fallback}
        {@render fallback(error, reset)}
    {:else}
        <div class="p-4 bg-destructive/10 border border-destructive rounded-lg">
            <h3 class="font-semibold text-destructive">Something went wrong</h3>
            <p class="text-sm text-muted-foreground mt-1">{error.message}</p>
            <button onclick={reset} class="mt-2 text-sm underline">Try again</button>
        </div>
    {/if}
{:else}
    {@render children()}
{/if}
```

**Usage:**
```svelte
<ErrorBoundary>
    {#snippet fallback(error, reset)}
        <div class="error-state">
            <p>Editor crashed: {error.message}</p>
            <button onclick={reset}>Reload Editor</button>
        </div>
    {/snippet}

    <Editor />
</ErrorBoundary>
```

**Implementation:**
- [ ] Create ErrorBoundary component
- [ ] Add to App.svelte wrapping main content
- [ ] Create specialized boundaries for Editor, Chat, etc.
- [ ] Integrate with error reporting service

---

### 10. Delta Compression for Checkpoints

**Problem:** Checkpoints store full content copies, wasting storage for small edits.

**Solution:** Implement delta compression using the `similar` crate.

```rust
use similar::{ChangeTag, TextDiff};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeltaOp {
    Keep(usize),      // Keep N characters from base
    Insert(String),   // Insert new content
    Delete(usize),    // Skip N characters from base
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaCheckpoint {
    pub id: String,
    pub base_hash: String,              // Reference to base content
    pub delta: Option<Vec<DeltaOp>>,    // None = identical to base
    pub timestamp: String,
    pub trigger: String,
    pub stats: CheckpointStats,
}

impl CheckpointManager {
    pub async fn create_delta_checkpoint(
        &mut self,
        file_path: &str,
        content: &str,
        trigger: &str,
    ) -> Result<DeltaCheckpoint> {
        let base_content = self.get_latest_content(file_path).await?;
        let base_hash = self.get_latest_hash(file_path)?;

        // If identical, store reference only
        if content == base_content {
            return Ok(DeltaCheckpoint {
                id: generate_checkpoint_id(),
                base_hash,
                delta: None,
                timestamp: Utc::now().to_rfc3339(),
                trigger: trigger.to_string(),
                stats: calculate_stats(content),
            });
        }

        // Compute delta
        let diff = TextDiff::from_chars(&base_content, content);
        let delta: Vec<DeltaOp> = diff.ops().iter().map(|op| {
            match op.tag() {
                ChangeTag::Equal => DeltaOp::Keep(op.old_range().len()),
                ChangeTag::Insert => {
                    let inserted: String = content[op.new_range()].to_string();
                    DeltaOp::Insert(inserted)
                },
                ChangeTag::Delete => DeltaOp::Delete(op.old_range().len()),
            }
        }).collect();

        // Only store delta if it's smaller than full content
        let delta_size = bincode::serialized_size(&delta)?;
        let content_size = content.len();

        if delta_size < content_size as u64 / 2 {
            Ok(DeltaCheckpoint {
                id: generate_checkpoint_id(),
                base_hash,
                delta: Some(delta),
                timestamp: Utc::now().to_rfc3339(),
                trigger: trigger.to_string(),
                stats: calculate_stats(content),
            })
        } else {
            // Fall back to full content for large changes
            let content_hash = self.object_store.write(content).await?;
            Ok(DeltaCheckpoint {
                id: generate_checkpoint_id(),
                base_hash: content_hash,
                delta: None,
                timestamp: Utc::now().to_rfc3339(),
                trigger: trigger.to_string(),
                stats: calculate_stats(content),
            })
        }
    }

    pub async fn apply_delta(&self, base_content: &str, delta: &[DeltaOp]) -> String {
        let mut result = String::new();
        let mut base_pos = 0;

        for op in delta {
            match op {
                DeltaOp::Keep(n) => {
                    result.push_str(&base_content[base_pos..base_pos + n]);
                    base_pos += n;
                }
                DeltaOp::Insert(s) => {
                    result.push_str(s);
                }
                DeltaOp::Delete(n) => {
                    base_pos += n;
                }
            }
        }

        result
    }
}
```

**Benefits:**
- 80-90% storage reduction for typical editing patterns
- Faster checkpoint creation (less I/O)
- Still supports full content restore
- Automatic fallback for large changes

**Implementation:**
- [ ] Add `similar` crate to Cargo.toml
- [ ] Implement DeltaCheckpoint type
- [ ] Update CheckpointManager with delta support
- [ ] Add migration for existing full checkpoints
- [ ] Update restore logic to apply deltas

---

## Current Progress

### Completed ✅

| Component | Status | Notes |
|-----------|--------|-------|
| Monorepo setup | ✅ | pnpm workspaces + Turborepo |
| `@midlight/core` types | ✅ | TiptapDocument, Checkpoint, StorageAdapter |
| Document serializer | ✅ | Tiptap JSON → Markdown (browser-compatible) |
| Document deserializer | ✅ | Markdown → Tiptap JSON (browser-compatible) |
| `@midlight/stores` | ✅ | fileSystem, ai, versions, auth, settings, agent |
| Web app scaffold | ✅ | SvelteKit + OPFS adapter |
| Desktop app scaffold | ✅ | Tauri + basic Svelte UI |
| Rust ObjectStore | ✅ | SHA-256 content-addressable storage |
| Rust CheckpointManager | ✅ | Version history with retention |
| Rust WorkspaceManager | ✅ | Service orchestration |
| Basic Tauri commands | ✅ | fs, workspace, versions, images |
| Tiptap extensions | ✅ | All 10 extensions ported to `@midlight/ui` |
| Editor toolbar | ✅ | Full formatting toolbar with color pickers, font size, image insert |
| Dirty state tracking | ✅ | isDirty, lastSavedAt, autoSave settings |
| Image manager (Rust) | ✅ | SHA-256 deduplication, save/load/delete |
| Image upload | ✅ | Tauri commands + editor toolbar button |
| Sidebar file management | ✅ | Context menu, multi-select, drag-drop, keyboard shortcuts |
| Version history UI | ✅ | VersionsPanel, SaveSnapshotModal, RestoreConfirmDialog, DiffViewer |
| RightSidebar tabs | ✅ | Chat/Versions tab system with auto-switching |
| LLM Client (Web) | ✅ | `@midlight/core` WebLLMClient with SSE streaming |
| LLM Client (Tauri) | ✅ | TauriLLMClient with event-based streaming |
| Rust LLM Service | ✅ | HTTP client, SSE parsing, multi-provider (OpenAI/Anthropic/Gemini) |
| Tauri LLM Commands | ✅ | chat, chatStream, chatWithTools, chatWithToolsStream, getModels, getQuota |
| AI Chat Panel | ✅ | Full UI with streaming, provider/model selection, temperature, web search |
| Context Picker | ✅ | @mention file picker with keyboard navigation |
| Agent Tools | ✅ | 7 tools: list, read, create, edit, move, delete, search |
| Agent Store | ✅ | Execution tracking, pending changes, confirmation flow |
| Agent Loop | ✅ | `sendMessageWithAgent()` with max 15 iterations |
| Rust Agent Executor | ✅ | All 7 tools implemented in `agent_executor.rs` |
| Thinking Steps UI | ✅ | Expandable steps with icons and status |
| Tool Actions UI | ✅ | ToolActionsGroup, ToolActionCard components |
| Pending Changes Panel | ✅ | Review UI with accept/reject, batch operations |
| Inline Editing | ✅ | Cmd+K trigger, InlineEditPrompt, streaming suggestions |
| Staged Edit Toolbar | ✅ | Accept/Reject floating toolbar for AI edits |
| AI Annotations | ✅ | Click-to-view popover, removal, change tracking |
| Auth Store | ✅ | User, Subscription, Quota types + methods |
| Auth Client (TS) | ✅ | init, login, signup, loginWithGoogle, getAccessToken |
| Auth Service (Rust) | ✅ | 648 lines: token management, OAuth, cookies |
| Auth Commands | ✅ | 12 Tauri commands for all auth operations |
| AuthModal UI | ✅ | Login/signup/forgot-password modes, Google OAuth button |
| Chat Auth Gating | ✅ | Sign-in required prompt in ChatPanel |
| Settings Auth Section | ✅ | Account display, sign out, profile editing, subscription tier |
| Subscription Store | ✅ | SubscriptionStatus, QuotaInfo, Price types, derived stores |
| Subscription Client | ✅ | fetchStatus, fetchQuota, fetchPrices, openCheckout, openPortal |
| Subscription Commands | ✅ | 3 Tauri commands: get_prices, create_checkout, create_portal |
| UpgradeModal UI | ✅ | Plan comparison, Stripe checkout trigger |
| QuotaExceededModal | ✅ | Usage limit reached, upgrade prompt |
| Quota Badge & Warning | ✅ | QuotaBadge, QuotaWarningBanner in ChatPanel |
| Quota Enforcement | ✅ | Blocks chat at limit, refresh after messages |
| Post-checkout Refresh | ✅ | Window focus listener refreshes subscription |
| Profile Management | ✅ | Edit name/email/password in SettingsModal |
| Forgot Password | ✅ | Email-based password reset flow |
| Import Service | ✅ | Obsidian/Notion import with wizard UI |
| Export Service | ✅ | DOCX export via docx-rs, PDF via webview print |
| Recovery Manager | ✅ | Hybrid WAL with 2s debounce, xxHash, RecoveryDialog UI |
| File Watcher | ✅ | Native events via notify crate, ExternalChangeDialog UI |
| Toast Notifications | ✅ | ToastContainer with auto-dismiss, pause on hover |
| Document Search | ✅ | Enhanced SearchDropdown with scoring, Ctrl+K/Ctrl+P |
| Error Reporting | ✅ | Opt-in, PII sanitization, rate limiting (50/session) |
| Keyboard Shortcuts | ✅ | Registry pattern, platform-aware modifiers, settings view |

### In Progress 🔄

| Component | Status | Remaining Work |
|-----------|--------|----------------|
| Document serialization integration | 🔄 | TypeScript serializers exist, need frontend integration |

### Not Started ❌

| Component | Priority |
|-----------|----------|
| Auto-updater | P2 |

---

## Feature Inventory

### IPC Operations (95 total)

Based on the existing Electron app's `preload.ts`, here are all operations that need to be implemented:

#### File System Operations (10)
- [ ] `selectDirectory()` - Native directory picker
- [ ] `selectFile()` - Native file picker
- [ ] `readDir(path)` - List directory contents
- [ ] `readFile(path)` - Read file content
- [ ] `fileExists(path)` - Check existence
- [ ] `readImageAsDataUrl(path)` - Image to base64
- [ ] `writeFile(path, content)` - Write file
- [ ] `createFolder(path)` - Create directory
- [ ] `deleteFile(path)` - Delete file/directory
- [ ] `renameFile(oldPath, newPath)` - Rename with sidecar handling

#### File Browser Context Menu (7) ✅ COMPLETE
- [x] `fileDuplicate(path)` - Duplicate file
- [x] `fileTrash(path)` - Move to trash
- [x] `fileRevealInFinder(path)` - Show in explorer
- [x] `fileCopyPath(path)` - Copy path to clipboard (via clipboard-manager plugin)
- [x] `folderCreate(parentPath, name)` - Create folder
- [x] `fileCopyTo(sourcePaths[], destDir)` - Copy files
- [x] `fileMoveTo(sourcePaths[], destDir)` - Move files

#### App Initialization (1) - NEW
- [x] `getDefaultWorkspace()` - Get/create default workspace (~/Documents/Midlight-docs)

#### Document Import/Export (4)
- [ ] `importDocx()` - DOCX import (not yet implemented)
- [ ] `importDocxFromPath(filePath)` - DOCX from path (not yet implemented)
- [x] `exportPdf()` - PDF export (via webview print)
- [x] `exportDocx(content)` - DOCX export

#### Workspace Operations (12)
- [x] `workspaceInit(root)` - Initialize workspace
- [x] `workspaceLoadDocument(root, filePath)` - Load document
- [ ] `workspaceLoadFromRecovery(root, filePath)` - Load recovery
- [ ] `workspaceDiscardRecovery(root, filePath)` - Discard recovery
- [x] `workspaceSaveDocument(root, filePath, json, trigger)` - Save document
- [ ] `workspaceStopWatcher(root)` - Stop file watcher
- [ ] `workspaceHasExternalChange(root, filePath)` - Check external changes

#### Versioning Operations (6)
- [x] `workspaceGetCheckpoints(root, filePath)` - Get versions
- [ ] `workspaceGetCheckpointContent(root, filePath, checkpointId)` - Get version content
- [x] `workspaceRestoreCheckpoint(root, filePath, checkpointId)` - Restore version
- [x] `workspaceCreateBookmark(root, filePath, json, label, description)` - Create named version
- [ ] `workspaceLabelCheckpoint(root, filePath, checkpointId, label)` - Label version
- [x] `workspaceCompareCheckpoints(root, filePath, idA, idB)` - Compare versions

#### Storage & Metadata (6)
- [ ] `workspaceGetImageDataUrl(root, imageRef)` - Get image
- [ ] `workspaceCheckForRecovery(root)` - Scan for recovery
- [ ] `workspaceGetStorageStats(root)` - Storage stats
- [ ] `workspaceRunGC(root)` - Garbage collection
- [ ] `workspaceGetConfig(root)` - Get config
- [ ] `workspaceUpdateConfig(root, updates)` - Update config

#### Import from Obsidian/Notion (7)
- [ ] `importSelectFolder()` - Select import source
- [ ] `importDetectSourceType(folderPath)` - Detect source
- [ ] `importAnalyzeObsidian(vaultPath)` - Analyze vault
- [ ] `importObsidian(analysisJson, destPath, optionsJson)` - Import vault
- [ ] `importAnalyzeNotion(exportPath)` - Analyze Notion
- [ ] `importNotion(analysisJson, destPath, optionsJson)` - Import Notion
- [ ] `importCancel()` - Cancel import

#### Authentication (12) ✅ COMPLETE
- [x] `auth.init()` - Silent refresh on app start (`auth_init`)
- [x] `auth.signup(email, password, displayName)` - Sign up (`auth_signup`)
- [x] `auth.login(email, password)` - Login (`auth_login`)
- [x] `auth.logout()` - Logout (`auth_logout`)
- [x] `auth.loginWithGoogle()` - Google OAuth (`auth_login_with_google` with local TCP callback)
- [x] `auth.handleOAuthCallback(code)` - OAuth code exchange (`auth_handle_oauth_callback`)
- [x] `auth.getUser()` - Get user profile (`auth_get_user`)
- [x] `auth.getSubscription()` - Get subscription (`auth_get_subscription`)
- [x] `auth.getQuota()` - Get usage quota (`auth_get_quota`)
- [x] `auth.isAuthenticated()` - Check auth (`auth_is_authenticated`)
- [x] `auth.getState()` - Get auth state (`auth_get_state`)
- [x] `auth.getAccessToken()` - Get token for API calls (`auth_get_access_token`)
- [x] `auth.onAuthStateChange(callback)` - Auth state listener (via Tauri events)

#### LLM Operations (11) ✅ COMPLETE
- [x] `llm.chat(options)` - Non-streaming chat (`llm_chat` Tauri command)
- [x] `llm.chatStream(options, channelId)` - Streaming chat (`llm_chat_stream` + events)
- [x] `llm.onStreamChunk(channelId, callback)` - Stream chunk listener (via Tauri events)
- [x] `llm.onStreamDone(channelId, callback)` - Stream done listener (via Tauri events)
- [x] `llm.onStreamUsage(channelId, callback)` - Usage listener (via Tauri events)
- [x] `llm.onStreamError(channelId, callback)` - Error listener (via Tauri events)
- [x] `llm.offStream(channelId)` - Remove listeners (via unlisten)
- [x] `llm.chatWithTools(options)` - Tool calling (`llm_chat_with_tools`)
- [x] `llm.getModels()` - Get available models (`llm_get_models`)
- [x] `llm.getQuota()` - Get quota (`llm_get_quota`)
- [x] `llm.getStatus()` - Get service status (`llm_get_status`)

#### Agent Operations (4) ✅ COMPLETE
- [x] `agent.getTools()` - Get tool definitions (in `@midlight/core/agent/tools.ts`)
- [x] `agent.executeTools(workspaceRoot, toolCalls[])` - Execute tools (`agent_execute_tool` Tauri command)
- [x] `agent.isDestructive(toolName)` - Check destructive (in tool definitions)
- [x] `agent.isReadOnly(toolName)` - Check read-only (in tool definitions)

#### Subscription Operations (4) ✅ COMPLETE
- [x] `subscription.getStatus()` - Get subscription (`auth_get_subscription`)
- [x] `subscription.createCheckout(priceType, urls)` - Stripe checkout (`subscription_create_checkout`)
- [x] `subscription.createPortal(returnUrl)` - Customer portal (`subscription_create_portal`)
- [x] `subscription.getPrices()` - Get pricing (`subscription_get_prices`)

#### Auto-Update Operations (7)
- [ ] `checkForUpdates()` - Check for updates
- [ ] `downloadUpdate()` - Download update
- [ ] `quitAndInstall()` - Install update
- [ ] `getAppVersion()` - Get version
- [ ] `onUpdateAvailable(callback)` - Update available
- [ ] `onUpdateDownloadProgress(callback)` - Download progress
- [ ] `onUpdateDownloaded(callback)` - Download complete

#### System Operations (7)
- [ ] `platform` - Get platform info
- [ ] `updateTitleBarOverlay()` - Update titlebar (Windows)
- [ ] `onMenuAction(callback)` - Menu action listener
- [ ] `onUpdateTheme(callback)` - Theme change listener
- [ ] `onShowLoginPrompt(callback)` - Login prompt
- [ ] `openExternal(url)` - Open in browser
- [ ] `onFileChangedExternally(callback)` - File change listener

---

### Backend Services (16 total)

| Service | Electron | Tauri | Web | Status |
|---------|----------|-------|-----|--------|
| WorkspaceManager | TS | Rust | TS | ✅ Complete |
| CheckpointManager | TS | Rust | TS | ✅ Complete |
| ObjectStore | TS | Rust | TS (OPFS) | ✅ Complete |
| ImageManager | TS | Rust | TS (OPFS) | ✅ Complete |
| RecoveryManager | TS | Rust | TS (IndexedDB) | ❌ |
| FileWatcher | TS | Rust (notify) | N/A | ❌ |
| DocumentSerializer | TS | TS (shared) | TS (shared) | ✅ |
| DocumentDeserializer | TS | TS (shared) | TS (shared) | ✅ |
| AuthService | TS | Rust + TS | TS (shared) | ✅ Complete |
| LLMService | TS | Rust + TS | TS (shared) | ✅ Complete |
| AgentExecutor | TS | Rust + TS | TS (shared) | ✅ Complete |
| SubscriptionService | TS | Rust + TS | TS (shared) | ✅ Complete |
| ImportService | TS | Rust + TS | TS | ❌ |
| AutoUpdateService | TS | Tauri plugin | N/A | ❌ |
| ErrorReportingService | TS | TS (shared) | TS (shared) | ❌ |
| SidecarManager | TS | TS (shared) | TS (shared) | ✅ (in serializer) |

---

### UI Components (45+ total)

#### Core Editor Components
- [ ] `Editor.svelte` - Tiptap wrapper with all features
- [ ] `EditorToolbar.svelte` - Formatting toolbar
- [ ] `BlockTypeDropdown.svelte` - Paragraph/heading selector
- [ ] `FontFamilyDropdown.svelte` - Font selector
- [ ] `FontSizeDropdown.svelte` - Size selector
- [ ] `ColorPickerDropdown.svelte` - Text color
- [ ] `HighlightPickerDropdown.svelte` - Highlight color
- [ ] `AlignmentDropdown.svelte` - Text alignment
- [ ] `ImageNodeView.svelte` - Resizable images

#### Tiptap Extensions (10) ✅ COMPLETE
- [x] `AIAnnotation.ts` - AI edit markers
- [x] `FontSize.ts` - Font size control
- [x] `DiffAdded.ts` - Added text highlighting
- [x] `DiffRemoved.ts` - Removed text highlighting
- [x] `PageSplitting.ts` - Paginated view
- [x] `ResizableImage.ts` - Image resizing (vanilla JS NodeView)
- [x] `TextColor.ts` - Text coloring
- [x] `TextHighlight.ts` - Text highlighting
- [x] `CustomCode.ts` - Code blocks
- [x] `ClickableHorizontalRule.ts` - HR node

*Location: `packages/ui/src/lib/extensions/`*

#### Sidebar Components ✅ COMPLETE
- [x] `Sidebar.svelte` - File tree with multi-select, drag-drop, inline rename
- [x] `FileContextMenu.svelte` - Right-click menu with all file operations
- [x] `ConfirmDialog.svelte` - Generic confirmation modal

*Location: `apps/desktop/src/lib/components/`*

#### Chat Components ✅ COMPLETE
- [x] `ChatPanel.svelte` - Full chat UI with streaming, provider/model selection, temperature, web search toggle
- [x] `ConversationTabs.svelte` - Multi-conversation tabs with create/delete
- [x] `ContextPicker.svelte` - @mention file picker with keyboard navigation
- [x] `ContextPills.svelte` - Display selected context items
- [x] `ThinkingSteps.svelte` - Expandable AI reasoning display with icons
- [x] `ToolActionsGroup.svelte` - Tool execution group display
- [x] `ToolActionCard.svelte` - Individual tool execution status
- [x] `PendingChangesPanel.svelte` - Review pending AI edits with accept/reject

*Location: `apps/desktop/src/lib/components/Chat/`*

#### Editor AI Components ✅ COMPLETE
- [x] `InlineEditPrompt.svelte` - Cmd+K floating prompt for inline edits
- [x] `InlineDiff.svelte` - Before/after comparison view
- [x] `StagedEditToolbar.svelte` - Accept/Reject toolbar for AI changes
- [x] `AnnotationPopover.svelte` - AI annotation display (UI ready, integration pending)

*Location: `apps/desktop/src/lib/components/Editor/`*

#### Version Components ✅ COMPLETE
- [x] `VersionsPanel.svelte` - Version list with selection, restore, compare
- [x] `DiffViewer.svelte` - Unified/split diff view in modal
- [x] `SaveSnapshotModal.svelte` - Create bookmark with label/description
- [x] `RestoreConfirmDialog.svelte` - Restore confirmation with backup option

*Location: `apps/desktop/src/lib/components/`*

#### Modal Components
- [ ] `AuthModal.svelte` - Login/signup
- [ ] `SettingsModal.svelte` - App settings
- [ ] `ImportWizard.svelte` - Import flow
- [ ] `UpgradeModal.svelte` - Subscription upgrade
- [ ] `QuotaExceededModal.svelte` - Usage limit
- [ ] `RecoveryPrompt.svelte` - Crash recovery
- [ ] `ExternalChangeDialog.svelte` - File conflicts

#### Layout Components
- [ ] `TitleBar.svelte` - Window title
- [ ] `TabBar.svelte` - Open file tabs
- [ ] `WelcomeScreen.svelte` - Initial view

#### Common Components
- [ ] `Button.svelte`
- [ ] `Dialog.svelte`
- [ ] `Dropdown.svelte`
- [ ] `Toast.svelte`
- [ ] `Tooltip.svelte`

---

### Svelte Stores (6)

| Store | Fields | Status |
|-------|--------|--------|
| fileSystem | rootDir, files, openFiles, activeFilePath, editorContent, isDirty, lastSavedAt, autoSaveEnabled, autoSaveInterval, hasRecovery, pendingDiffs, selectedPaths, clipboardPaths, clipboardOperation, stagedEdit | ✅ Complete |
| ai | conversations, activeConversationId, isStreaming, error, selectedProvider, selectedModel, contextItems, webSearchEnabled, inlineEdit, annotationsVisible + methods: sendMessage, sendMessageWithAgent, sendInlineEditRequest, acceptInlineEdit, setLLMClient, setToolExecutor | ✅ Complete |
| versions | isOpen, versions, selectedVersionId, isLoading + methods: open, close, setVersions, selectVersion, setIsLoading | ✅ Complete |
| auth | user, subscription, quota, isAuthenticated, isInitializing, error + methods: setUser, setSubscription, setQuota, logout | ✅ Complete |
| settings | isOpen, activeTab, theme, font, fontSize | 🔄 Partial |
| agent | toolExecutions, pendingChanges + methods: startExecution, completeExecution, addPendingChange, acceptChange, rejectChange, requireConfirmation | ✅ Complete |

---

## Migration Phases

### Phase 1: Core Editor (P0) - Weeks 1-3

**Goal:** Functional rich-text editing with basic formatting

**Status:** ✅ Complete

#### Tasks
1. ✅ Port all Tiptap extensions from React
2. ✅ Build complete EditorToolbar with all formatting options
3. ✅ Implement image upload and resizing
4. ✅ Add paginated view mode (PageSplitting extension done)
5. ✅ Implement auto-save with dirty state

#### Completed Work
- **Extensions ported** (`packages/ui/src/lib/extensions/`):
  - FontSize, TextColor, TextHighlight
  - DiffAdded, DiffRemoved
  - AIAnnotation
  - CustomCode, ClickableHorizontalRule
  - ResizableImage (vanilla JS NodeView with drag handles)
  - PageSplitting
- **Editor toolbar** with:
  - Undo/Redo
  - Bold, Italic, Underline, Strikethrough
  - Font size dropdown (12px-36px)
  - Text color picker (16 colors)
  - Highlight color picker (12 colors)
  - Headings (H1, H2, H3)
  - Lists (bullet, numbered)
  - Blockquote
  - Code (inline, block)
  - Horizontal rule
  - Image insert button
  - Text alignment
  - Clear formatting
- **Dirty state tracking** in fileSystem store:
  - `isDirty`, `lastSavedAt`
  - `autoSaveEnabled`, `autoSaveInterval`
  - `setAutoSave()` method
- **Image management** (Rust):
  - `ImageManager` service with SHA-256 deduplication
  - Tauri commands: `workspace_save_image`, `workspace_get_image`, `workspace_image_exists`, `workspace_delete_image`, `workspace_list_images`
  - Images stored in `.midlight/images/` with hash-based filenames

#### Success Criteria
- [x] All formatting options work (bold, italic, underline, colors, fonts)
- [x] Images can be inserted and resized
- [x] Documents save correctly as Markdown + sidecar (basic flow working)
- [x] Editor loads existing documents with full formatting

---

### Phase 2: File Management (P0) - COMPLETE

**Goal:** Complete workspace and file operations

#### Tasks - ALL COMPLETE
1. ✅ Enhance Sidebar with context menu - FileContextMenu.svelte
2. ✅ Implement file operations (create, rename, delete, duplicate) - Rust commands + UI
3. ✅ Add drag-and-drop file moving - Native HTML5 drag/drop with move_to
4. ✅ Implement file watcher (Tauri) for external changes - Completed in Phase 7
5. ✅ Add multi-file selection - Ctrl+Click toggle, Shift+Click range
6. ⏸️ Build TabBar for multiple open files - Already functional from Phase 1

#### Implementation Notes
- Added 6 new Rust commands: file_duplicate, file_trash, file_reveal, file_copy_to, file_move_to, get_default_workspace
- Added clipboard-manager plugin for "Copy Path" functionality
- Added `dirs` crate for cross-platform Documents directory detection
- Delete uses OS trash (trash crate) instead of permanent deletion
- Context menu supports single and multi-selection variants
- Keyboard shortcuts: Delete, F2, Cmd+C/X/V, Escape
- File type icons: markdown (blue), importable (orange), viewable (green), generic (gray)
- ConfirmDialog.svelte for delete confirmation
- **Default workspace auto-creation**: App automatically creates and opens `~/Documents/Midlight-docs` on startup
- **AI store fixes**: Added `error` state, `setStreaming()`, `setError()`, `clearConversation()` methods
- **RightSidebar fixes**: Updated to use `activeConversation` derived store instead of non-existent `$ai.messages`

#### Success Criteria
- [ ] All file operations work
- [ ] External file changes detected and handled
- [ ] Multiple files can be open in tabs
- [ ] File tree supports expand/collapse with state

---

### Phase 3: Version History (P1) - COMPLETE

**Goal:** Full versioning with bookmarks and restore

**Status:** ✅ Complete

#### Tasks - ALL COMPLETE
1. ✅ Complete CheckpointManager in Rust - Already done in previous phases
2. ✅ Build VersionsPanel UI - Full version list with selection, icons, timestamps
3. ⏸️ Implement version preview - Deferred (nice-to-have)
4. ✅ Add version comparison (side-by-side diff) - DiffViewer with unified/split modes
5. ✅ Build SaveSnapshotModal for creating bookmarks - Label, description, validation
6. ✅ Add version restore functionality - RestoreConfirmDialog with backup option

#### Implementation Notes
- **Tab system**: RightSidebar now has Chat/Versions tabs, auto-switches to Versions when `versions.isOpen`
- **New components created**:
  - `ChatPanel.svelte` - Extracted AI chat from RightSidebar
  - `VersionsPanel.svelte` - Version list with selection, restore, compare buttons
  - `SaveSnapshotModal.svelte` - Create bookmarks with label (required) and description (optional)
  - `RestoreConfirmDialog.svelte` - Confirm restore with "Create backup first" checkbox
  - `DiffViewer.svelte` - Compare versions with unified/split view modes
- **Version list features**:
  - Relative timestamps ("5m ago", "2h ago", etc.)
  - Change size indicators (+15 or -20 chars)
  - Star icon for bookmarks, clock icon for auto-saved
  - Click to select, shows Restore/Compare buttons
- **Compare mode**: Calls `compare_checkpoints` Tauri command, displays in modal with DiffViewer
- **Restore flow**: Shows warning, optionally creates backup, updates editor content

#### Success Criteria
- [x] Auto-checkpoints created on save (backend already working)
- [x] Users can create named versions (bookmarks)
- [ ] Versions can be previewed without switching (deferred)
- [x] Two versions can be compared side-by-side
- [x] Any version can be restored

---

### Phase 4: AI Integration (P0) - ✅ COMPLETE

**Goal:** Full AI chat with agent tools

**Status:** ✅ All features implemented.

#### Tasks
1. ✅ Build LLM service connecting to midlight.ai/api/llm
2. ✅ Implement streaming chat responses
3. ✅ Build AIChatPanel with message history
4. ✅ Add @ mention context picker
5. ✅ Implement AI Agent executor with 7 tools
6. ✅ Build pending changes review UI
7. ✅ Add inline editing mode
8. ✅ Implement AI annotations

#### Implementation Notes
- **LLM Architecture:**
  - `@midlight/core/llm`: Types + WebLLMClient (browser reference)
  - `apps/desktop/src/lib/llm.ts`: TauriLLMClient with event-based streaming
  - `src-tauri/services/llm_service.rs`: HTTP client with SSE parsing
  - `src-tauri/commands/llm.rs`: 6 commands (chat, stream, tools, models, quota, status)
- **Agent Architecture:**
  - `@midlight/core/agent/tools.ts`: 7 tool definitions with JSON Schema
  - `@midlight/stores/agent.ts`: Execution tracking, pending changes, confirmations
  - `@midlight/stores/ai.ts`: `sendMessageWithAgent()` loop (max 15 iterations)
  - `src-tauri/services/agent_executor.rs`: Rust tool implementations
- **Chat UI Components** (`apps/desktop/src/lib/components/Chat/`):
  - `ContextPicker.svelte`: @mention file picker with keyboard nav
  - `ContextPills.svelte`: Display selected context items
  - `ThinkingSteps.svelte`: Agent reasoning visualization
  - `ToolActionsGroup.svelte`, `ToolActionCard.svelte`: Tool execution display
  - `PendingChangesPanel.svelte`: Accept/reject pending edits
  - `ConversationTabs.svelte`: Multi-conversation support
- **Inline Editing** (`apps/desktop/src/lib/components/Editor/`):
  - Cmd+K shortcut triggers `InlineEditPrompt.svelte`
  - `InlineDiff.svelte`: Before/after comparison
  - `StagedEditToolbar.svelte`: Accept/Reject floating toolbar
  - `ai.sendInlineEditRequest()`: Streaming LLM call for edits
- **Annotations** (complete):
  - `AnnotationPopover.svelte`: Click-to-view UI with removal
  - AIAnnotation Tiptap extension with setAIAnnotation/unsetAIAnnotation
  - Change tracking via `computeChangeRanges()` in diff.ts
  - Annotations applied on staged edit accept and inline edit accept

#### Success Criteria
- [x] Chat works with streaming responses
- [x] @ mentions add file context
- [x] Agent can create, edit, delete, move documents
- [x] Changes require user review before applying
- [x] Undo capability for agent changes (via staged edits)
- [x] AI annotations visible in editor with click-to-view and removal

---

### Phase 5: Authentication & Subscription (P1) - ✅ COMPLETE

**Goal:** User accounts with subscription management

**Status:** ✅ All code implemented. Only backend Stripe configuration and end-to-end testing remain.

#### Tasks
1. ✅ Build AuthService with JWT/refresh tokens (Rust: `auth_service.rs` 876 lines)
2. ✅ Implement email/password login/signup (`auth_login`, `auth_signup` commands)
3. ✅ Add Google OAuth flow (local TCP callback server + event-driven)
4. ✅ Build AuthModal UI (`AuthModal.svelte` with login/signup/forgot-password modes)
5. ✅ Connect SubscriptionService to Stripe (all commands implemented)
6. ✅ Build UpgradeModal with pricing (`UpgradeModal.svelte` with plan selection)
7. ✅ Add quota tracking and limits (full enforcement in ChatPanel)

#### Implementation Notes
- **Auth Store** (`packages/stores/src/auth.ts`):
  - User, Subscription, Quota types
  - `isAuthenticated`, `isInitializing` state
  - `setUser()`, `setSubscription()`, `logout()` methods
- **Subscription Store** (`packages/stores/src/subscription.ts`):
  - SubscriptionStatus, QuotaInfo, Price types
  - Derived stores: `isFreeTier`, `quotaPercentUsed`, `isQuotaExceeded`, `showQuotaWarning`
  - Full state management for subscription flow
- **Auth Client** (`apps/desktop/src/lib/auth.ts`):
  - `init()` - Silent refresh on app start
  - `login(email, password)`, `signup()`, `loginWithGoogle()`
  - `forgotPassword(email)`, `resetPassword(token, password)`
  - `updateProfile({ displayName, email, currentPassword, newPassword })`
  - `getAccessToken()` - Used by LLM client
  - Event listeners for OAuth completion
- **Subscription Client** (`apps/desktop/src/lib/subscription.ts`):
  - `fetchStatus()`, `fetchQuota()`, `fetchPrices()`
  - `openCheckout(priceId)` - Opens Stripe checkout
  - `openPortal()` - Opens Stripe billing portal
  - `refresh()` - Refreshes all subscription data
- **Rust AuthService** (`src-tauri/services/auth_service.rs` - 876 lines):
  - In-memory access token (never persisted)
  - Refresh token in httpOnly cookies
  - 60-second early refresh buffer
  - OAuth code exchange
  - `get_prices()`, `create_checkout_session()`, `create_portal_session()`
  - `forgot_password()`, `reset_password()`, `update_profile()`
- **Tauri Commands** (`src-tauri/commands/auth.rs`):
  - 15 auth commands: init, signup, login, logout, login_with_google, handle_oauth_callback, get_user, get_subscription, get_quota, is_authenticated, get_state, get_access_token, forgot_password, reset_password, update_profile
  - 3 subscription commands: subscription_get_prices, subscription_create_checkout, subscription_create_portal
- **UI Integration**:
  - `AuthModal.svelte` - Login/signup/forgot-password modes, Google OAuth
  - `UpgradeModal.svelte` - Plan comparison, Stripe checkout
  - `QuotaExceededModal.svelte` - Usage limit reached with upgrade prompt
  - `SettingsModal.svelte` - Account section with profile editing, billing portal access
  - `QuotaBadge.svelte`, `QuotaWarningBanner.svelte` - Quota display in ChatPanel
  - Sign-in gate in ChatPanel
  - Quota enforcement in `handleSubmit()` with refresh after messages
  - Subscription refresh on window focus for post-checkout detection

#### Backend Configuration Required
The client-side implementation is complete. Backend (midlight.ai) needs:
1. Configure Stripe Price IDs for Premium/Pro plans
2. Test checkout webhook flow
3. Verify billing portal session creation

#### Success Criteria
- [x] Users can sign up and login
- [x] Google OAuth works
- [x] Subscription status reflected in UI
- [x] Quota limits enforced (blocks chat when exceeded)
- [x] Quota warning banner at 75%+ usage
- [x] Upgrade modal with pricing and checkout
- [x] Billing portal access for paid users
- [x] Password reset flow
- [x] Profile editing (name, email, password)
- [x] Post-checkout subscription refresh

---

### Phase 6: Import/Export (P2)

**Goal:** Import from Obsidian/Notion, export to PDF/DOCX
**Status:** ✅ COMPLETE

**Latest Session (January 2025):** Completed full import/export implementation:
- ✅ Import security utilities (Rust) - path sanitization, YAML safety, validation
- ✅ Import transaction manager (Rust) - atomic operations with rollback
- ✅ Import service (Rust) - Obsidian/Notion analysis and import with wiki-link conversion
- ✅ Tauri commands - IPC handlers for import operations
- ✅ ImportWizard.svelte - 5-step import wizard UI
- ✅ ExportProgress.svelte - Export progress modal
- ✅ Import store and client - Frontend state management
- ✅ DOCX export (Rust) - Full Tiptap → DOCX conversion using docx-rs crate
- ✅ PDF export - Via webview print API
- ✅ Export client and UI wiring - Menu actions connected to export functionality

---

#### Overview

Phase 6 ports the comprehensive import/export system from the Electron app. The Electron implementation consists of:
- **importService.ts** (1591 lines) - Obsidian/Notion vault analysis and import
- **importSecurity.ts** (817 lines) - Path sanitization, YAML safety, validation
- **importTransaction.ts** - Atomic operations with staging directory and rollback
- **docx-worker.ts** (381 lines) - DOCX export in worker thread
- **docx-transformer.ts** - Tiptap JSON → DOCX conversion
- **ImportWizard.tsx** (643 lines) - Multi-step import wizard UI

---

#### 6.1 Rust Backend: Import Security (`src-tauri/src/services/import_security.rs`)

Security utilities must be implemented first as they're used by all import operations.

**Types:**
```rust
pub struct ImportConfig {
    pub max_content_size: usize,        // 10MB for regex processing
    pub max_path_length: usize,         // 1000 chars
    pub max_filename_length: usize,     // 255 chars
    pub max_yaml_size: usize,           // 1MB
    pub max_yaml_aliases: usize,        // 100 (billion laughs protection)
    pub max_yaml_depth: usize,          // 50 levels
    pub parallel_batch_size: usize,     // 10 files
    pub progress_throttle_ms: u64,      // 100ms
}

pub enum AllowedExtension {
    Markdown,   // .md, .markdown, .mdown, .mkd
    Image,      // .png, .jpg, .jpeg, .gif, .webp, .svg, .bmp, .ico
    Attachment, // .pdf, .mp3, .mp4, .wav, .mov, .webm, .ogg
    Data,       // .csv, .json
}
```

**Functions to implement:**
- [x] `sanitize_filename(filename: &str) -> Result<String, ImportError>`
  - Remove null bytes, control characters
  - Handle Windows reserved names (CON, PRN, AUX, NUL, etc.)
  - Remove trailing dots/spaces
  - Enforce max length (255 chars)

- [x] `sanitize_relative_path(path: &str) -> Result<PathBuf, ImportError>`
  - Decode URL encoding (prevent %2e%2e bypass)
  - Normalize Unicode (NFC)
  - Reject absolute paths
  - Remove `..` and `.` segments
  - Validate each component

- [x] `is_path_safe(dest: &Path, base: &Path) -> bool`
  - Verify resolved path stays within base directory

- [x] `validate_path(path: &str) -> Result<(), ImportError>`
  - Check for null bytes, length limits, control chars

- [x] `safe_parse_yaml(content: &str) -> Result<Value, ImportError>`
  - Size limits (1MB max)
  - Use serde_yaml with safe settings
  - Depth/alias limits

- [x] `safe_parse_front_matter(content: &str) -> Result<Option<FrontMatter>, ImportError>`
  - Extract YAML between `---` delimiters
  - Parse safely with limits

- [x] `is_external_url(url: &str) -> bool`
  - Allowlist: http:, https:, mailto:

- [x] `is_dangerous_scheme(url: &str) -> bool`
  - Block: javascript:, data:, vbscript:, file:

- [x] `format_user_error(error: &std::io::Error) -> String`
  - EACCES → "Permission denied"
  - ENOENT → "File not found"
  - ENOSPC → "Insufficient disk space"

**Crate dependencies:**
- `unicode-normalization` - Unicode NFC normalization
- `serde_yaml` - YAML parsing (with custom deserializer for depth limits)
- `url` - URL parsing and validation

---

#### 6.2 Rust Backend: Import Transaction (`src-tauri/src/services/import_transaction.rs`)

Atomic operations with rollback capability.

**Struct:**
```rust
pub struct ImportTransaction {
    staging_dir: PathBuf,    // .import-staging-{timestamp}-{random}
    dest_path: PathBuf,
    staged_files: Vec<PathBuf>,
    committed: bool,
}
```

**Methods:**
- [x] `new(dest_path: PathBuf) -> Result<Self, ImportError>`
  - Create staging directory with random suffix

- [x] `stage_file(&mut self, relative_path: &Path, content: &[u8]) -> Result<(), ImportError>`
  - Write file to staging directory
  - Track for commit/rollback

- [x] `stage_copy(&mut self, source: &Path, relative_path: &Path) -> Result<(), ImportError>`
  - Copy file to staging with path validation

- [x] `verify_copy(&self, source: &Path, staged: &Path) -> Result<bool, ImportError>`
  - SHA-256 checksum verification for large files (>10MB)

- [x] `commit(&mut self) -> Result<TransactionStats, ImportError>`
  - Atomically move all files to final location
  - Mark as committed

- [x] `rollback(&mut self) -> Result<(), ImportError>`
  - Delete staging directory and all contents
  - Safe to call multiple times

- [x] `validate_disk_space(dest: &Path, required: u64) -> Result<(), ImportError>`
  - Check available space
  - Require 10% buffer beyond actual size

**Drop implementation:**
- Auto-rollback if not committed (RAII pattern)

---

#### 6.3 Rust Backend: Import Service (`src-tauri/src/services/import_service.rs`)

Core import logic for Obsidian and Notion.

**Types:**
```rust
pub enum ImportSourceType {
    Obsidian,
    Notion,
    Generic,
}

pub struct ImportAnalysis {
    pub source_type: ImportSourceType,
    pub source_path: PathBuf,
    pub total_files: usize,
    pub markdown_files: usize,
    pub attachments: usize,
    pub folders: usize,
    pub wiki_links: usize,
    pub files_with_wiki_links: usize,
    pub front_matter: usize,
    pub callouts: usize,
    pub dataview_blocks: usize,
    pub untitled_pages: Vec<String>,
    pub empty_pages: Vec<String>,
    pub files_to_import: Vec<ImportFileInfo>,
    pub access_warnings: Vec<AccessWarning>,
}

pub struct ImportFileInfo {
    pub source_path: PathBuf,
    pub relative_path: PathBuf,
    pub name: String,
    pub file_type: ImportFileType,
    pub size: u64,
    pub has_wiki_links: bool,
    pub has_front_matter: bool,
    pub has_callouts: bool,
    pub has_dataview: bool,
}

pub struct ImportOptions {
    pub convert_wiki_links: bool,
    pub import_front_matter: bool,
    pub convert_callouts: bool,
    pub copy_attachments: bool,
    pub preserve_folder_structure: bool,
    pub skip_empty_pages: bool,
    pub create_midlight_files: bool,
}

pub struct NotionImportOptions {
    #[serde(flatten)]
    pub base: ImportOptions,
    pub remove_uuids: bool,
    pub convert_csv_to_tables: bool,
    pub untitled_handling: UntitledHandling,
}

pub struct ImportProgress {
    pub phase: ImportPhase,
    pub current: usize,
    pub total: usize,
    pub current_file: String,
    pub errors: Vec<ImportError>,
    pub warnings: Vec<ImportWarning>,
}

pub struct ImportResult {
    pub success: bool,
    pub files_imported: usize,
    pub links_converted: usize,
    pub attachments_copied: usize,
    pub errors: Vec<ImportError>,
    pub warnings: Vec<ImportWarning>,
}
```

**Functions to implement:**

**Detection:**
- [x] `detect_source_type(folder_path: &Path) -> Result<ImportSourceType, ImportError>`
  - Check for `.obsidian` folder → Obsidian
  - Check for UUID-suffixed filenames → Notion
  - Otherwise → Generic

**Obsidian Analysis:**
- [x] `analyze_obsidian_vault(vault_path: &Path) -> Result<ImportAnalysis, ImportError>`
  - Recursively scan vault (parallel with rayon)
  - Count files by type
  - Detect wiki-links, callouts, dataview, front-matter
  - Track access warnings for unreadable files

**Notion Analysis:**
- [x] `analyze_notion_export(export_path: &Path) -> Result<ImportAnalysis, ImportError>`
  - Scan export folder
  - Count CSV databases
  - Track UUID-suffixed files

**Obsidian Import:**
- [x] `import_obsidian_vault(analysis: &ImportAnalysis, dest: &Path, options: &ImportOptions, progress_tx: Sender<ImportProgress>, cancel: CancellationToken) -> Result<ImportResult, ImportError>`
  - Use ImportTransaction for atomicity
  - Process files in parallel batches (rayon with limit)
  - 4 phases: converting → copying → finalizing → complete
  - Convert wiki-links to standard markdown links
  - Convert callouts to blockquotes
  - Remove dataview blocks
  - Preserve front-matter
  - Copy attachments with verification

**Content Conversion:**
- [x] `convert_wiki_links(content: &str, file_map: &HashMap<String, PathBuf>) -> (String, usize, Vec<BrokenLink>)`
  - Convert `[[link]]` to `[link](path.md)`
  - Handle `[[link|alias]]` syntax
  - Handle `[[link#heading]]` anchors
  - Build case-insensitive file map for resolution

- [x] `convert_callouts(content: &str) -> String`
  - Convert `> [!note]` to styled blockquotes
  - Preserve callout types (note, warning, tip, etc.)

- [x] `remove_dataview(content: &str) -> String`
  - Strip `dataview` and `dataviewjs` code blocks
  - Remove inline dataview expressions

- [x] `build_file_map(files: &[ImportFileInfo]) -> HashMap<String, PathBuf>`
  - Create case-insensitive lookup maps
  - Handle multiple name variants

**Notion Import:**
- [x] `import_notion_export(analysis: &ImportAnalysis, dest: &Path, options: &NotionImportOptions, progress_tx: Sender<ImportProgress>, cancel: CancellationToken) -> Result<ImportResult, ImportError>`
  - Strip UUIDs from filenames
  - Convert CSV to Markdown tables
  - Rebuild links after filename changes

- [x] `strip_notion_uuid(filename: &str) -> String`
  - Remove ` abc123def456` UUID suffixes

- [x] `csv_to_markdown_table(csv_content: &str) -> Result<String, ImportError>`
  - Parse CSV safely
  - Generate Markdown table
  - Handle CSV injection prevention

**Crate dependencies:**
- `rayon` - Parallel file processing
- `walkdir` - Recursive directory traversal
- `regex` - Content pattern matching
- `sha2` - SHA-256 checksums
- `csv` - CSV parsing
- `tokio-util` - CancellationToken

---

#### 6.4 Tauri Commands (`src-tauri/src/commands/import.rs`)

IPC handlers for import operations.

**Commands:**
- [x] `import_select_folder() -> Result<Option<String>, String>`
  - Open native folder picker dialog

- [x] `import_detect_source_type(folder_path: String) -> Result<ImportSourceType, String>`
  - Detect Obsidian/Notion/Generic

- [x] `import_analyze_obsidian(vault_path: String) -> Result<ImportAnalysis, String>`
  - Return full analysis for UI display

- [x] `import_obsidian(analysis_json: String, dest_path: String, options_json: String) -> Result<ImportResult, String>`
  - Validate JSON inputs
  - Start import with progress events
  - Return result when complete

- [x] `import_analyze_notion(export_path: String) -> Result<ImportAnalysis, String>`

- [x] `import_notion(analysis_json: String, dest_path: String, options_json: String) -> Result<ImportResult, String>`

- [x] `import_cancel() -> Result<(), String>`
  - Cancel active import via CancellationToken

**Event emissions:**
- [x] `import-progress` event with ImportProgress payload (throttled to 100ms)

---

#### 6.5 Svelte UI: Import Wizard (`packages/ui/src/components/ImportWizard.svelte`)

Multi-step import wizard component.

**State:**
```typescript
type ImportStep = 'select' | 'analyze' | 'options' | 'importing' | 'complete';

interface ImportWizardState {
  step: ImportStep;
  sourcePath: string | null;
  sourceType: ImportSourceType | null;
  analysis: ImportAnalysis | null;
  options: ImportOptions;
  progress: ImportProgress | null;
  result: ImportResult | null;
  error: string | null;
}
```

**Steps:**

1. **Select** - Folder selection
   - Browse button → native folder picker
   - DropZone for drag-and-drop
   - Display selected path

2. **Analyze** - Analysis results
   - Show file count breakdown (total, markdown, attachments)
   - List detected features (wiki-links, callouts, dataview)
   - Display access warnings if any
   - Show untitled/empty page warnings
   - "Import" / "Customize Options" buttons

3. **Options** - Conversion settings
   - Checkboxes for each option:
     - `convertWikiLinks` - Convert [[links]] to standard [links]()
     - `importFrontMatter` - Preserve YAML front-matter
     - `convertCallouts` - Convert Obsidian callout syntax
     - `copyAttachments` - Copy media files
     - `preserveFolderStructure` - Maintain folder hierarchy
     - `skipEmptyPages` - Don't import blank files
     - `createMidlightFiles` - Create .midlight metadata
   - Notion-specific options:
     - `removeUUIDs` - Strip UUID suffixes from filenames
     - `convertCSVToTables` - Convert CSV databases to Markdown tables
     - `untitledHandling` - How to handle untitled pages

4. **Importing** - Progress display
   - Phase indicator (Converting → Copying → Finalizing)
   - Progress bar with percentage
   - Current file name
   - Cancel button
   - Real-time error/warning list

5. **Complete** - Results
   - Success/failure status
   - File counts (imported, links converted, attachments)
   - Error list (if any)
   - Warning list (if any)
   - "Open Workspace" button

**Props:**
```typescript
interface ImportWizardProps {
  isOpen: boolean;
  onClose: () => void;
  initialPath?: string;  // For drag-drop pre-selection
  onComplete?: (result: ImportResult) => void;
}
```

---

#### 6.6 Svelte UI: Supporting Components

**ImportDetectionDialog.svelte** - Smart folder detection
- [ ] Shows when opening a folder that looks like Obsidian/Notion
- [ ] Offers: "Quick Import" / "Customize" / "Open without import"
- [ ] Passes to ImportWizard if import selected

**DropZone.svelte** - Drag-drop zone
- [ ] Desktop overlay on drag enter
- [ ] Extract folder path from dropped item
- [ ] Integrate with ImportWizard

**ExportProgress.svelte** - Export progress modal
- [x] Phase/percentage display
- [x] Loading animation
- [x] Error display on failure
- [x] Auto-close on success

---

#### 6.7 DOCX Import

**Approach:** Use `mammoth` via Tauri's Node.js sidecar or WASM.

**Option A: Node.js Sidecar** (Recommended for parity)
- Bundle minimal Node.js script with mammoth
- Call via Tauri's sidecar command
- Parse DOCX → HTML → Tiptap JSON

**Option B: Pure Rust** (Alternative)
- Use `docx-rs` crate for parsing
- Build custom HTML converter
- More work but no Node dependency

**Commands:**
- [ ] `import_docx() -> Result<{html: String, filename: String}, String>`
  - Open file dialog
  - Convert DOCX to HTML

- [ ] `import_docx_from_path(path: String) -> Result<{html: String, filename: String}, String>`
  - Import specific file (for sidebar drag)

**Frontend:**
- [ ] `ImportConfirmDialog.svelte` - Confirm individual file import
- [ ] HTML → Tiptap JSON conversion in frontend (using turndown + custom rules)

---

#### 6.8 PDF Export

**Approach:** Use Tauri's webview print API.

**Command:**
- [x] `export_pdf() -> Result<bool, String>`
  - Use `webview.print()` with PDF settings
  - A4 page size, 1-inch margins
  - Background printing enabled

**Frontend:**
- [x] Menu item: File → Export as PDF
- [x] Keyboard shortcut: Cmd/Ctrl+P

---

#### 6.9 DOCX Export

**Approach:** Use `docx-rs` crate in Rust (no worker thread needed - Rust is fast).

**Rust Service (`src-tauri/src/services/docx_export.rs`):**
```rust
pub fn tiptap_to_docx<F>(
    content: &TiptapDocument,
    progress_callback: F,
) -> Result<Vec<u8>, String>
where
    F: Fn(ExportProgress)
```

**Node mappings:**
- [x] `paragraph` → `Paragraph`
- [x] `heading` → `Paragraph` with HeadingLevel
- [x] `bulletList` → `Paragraph` with bullet numbering
- [x] `orderedList` → `Paragraph` with ordered numbering
- [x] `image` → Placeholder `[Image]` (full image support pending docx-rs API)
- [x] `horizontalRule` → Empty `Paragraph`

**Text run handling:**
- [x] Bold, italic, strike, underline marks
- [x] Font family mapping (web fonts → Word fallbacks)
- [x] Font size conversion (px → half-points)
- [x] Text color normalization
- [x] Highlight/shading (via color mapping)

**Crate dependencies:**
- `docx-rs` - DOCX generation

**Command:**
- [ ] `export_docx(content_json: String) -> Result<(), String>`
  - Parse Tiptap JSON
  - Generate DOCX buffer
  - Save dialog → write file

**Frontend:**
- [ ] Menu item: File → Export as Word Document
- [ ] ExportProgress.svelte for progress display

---

#### 6.10 Stores Integration (`packages/stores/src/import.ts`)

**Import Store:**
```typescript
interface ImportState {
  isImporting: boolean;
  currentImport: {
    sourcePath: string;
    sourceType: ImportSourceType;
    progress: ImportProgress | null;
  } | null;
  lastResult: ImportResult | null;
}

export const importStore = createImportStore();
// Methods: startImport, updateProgress, cancelImport, completeImport
```

**Export Store:**
```typescript
interface ExportState {
  isExporting: boolean;
  exportType: 'pdf' | 'docx' | null;
  progress: ExportProgress | null;
  error: string | null;
}

export const exportStore = createExportStore();
// Methods: startExport, updateProgress, completeExport, failExport
```

---

#### Implementation Order

1. **Foundation** (6.1-6.2)
   - Import security utilities
   - Import transaction manager

2. **Core Import** (6.3-6.4)
   - Import service with Obsidian support
   - Tauri commands
   - Basic testing with sample vault

3. **Import UI** (6.5-6.6)
   - ImportWizard component
   - Supporting dialogs
   - Integration with stores

4. **Notion Import** (6.3 continued)
   - Notion-specific analysis and import
   - CSV conversion
   - UUID stripping

5. **Export** (6.7-6.9)
   - DOCX import (sidecar or WASM)
   - PDF export via webview
   - DOCX export via docx-rs

6. **Polish**
   - Error handling and edge cases
   - Progress throttling
   - Cancellation cleanup

---

#### Success Criteria
- [x] Obsidian vaults import with wiki-links converted
- [x] Obsidian callouts converted to blockquotes
- [x] Front-matter preserved during import
- [x] Notion exports import with UUIDs stripped
- [x] CSV databases converted to Markdown tables
- [x] Import cancellation works cleanly
- [x] Atomic rollback on import failure
- [ ] DOCX files can be imported
- [ ] Documents export to PDF
- [ ] Documents export to DOCX with formatting

---

#### Testing Strategy

**Unit Tests (Rust):**
- Path sanitization edge cases
- Wiki-link conversion patterns
- Callout conversion
- UUID stripping
- CSV to Markdown

**Integration Tests:**
- Import sample Obsidian vault
- Import sample Notion export
- Verify file structure preserved
- Verify link resolution

**E2E Tests:**
- Full import wizard flow
- Export to PDF/DOCX
- Cancel mid-import

---

### Phase 7: Recovery & Polish (P1)

**Goal:** Crash recovery, error handling, file watching, search, and polish

**Status:** ✅ COMPLETE

**Implemented Files:**
- `src-tauri/src/services/recovery_manager.rs` - Hybrid WAL with xxHash (300 LOC)
- `src-tauri/src/services/file_watcher.rs` - Native events via notify crate (380 LOC)
- `src-tauri/src/services/error_reporter.rs` - PII sanitization, rate limiting (200 LOC)
- `src-tauri/src/commands/recovery.rs` - 8 Tauri IPC commands
- `src-tauri/src/commands/file_watcher.rs` - Start, stop, mark/clear saving commands
- `src-tauri/src/commands/error_reporter.rs` - Enable, status, report commands
- `packages/stores/src/recovery.ts` - Svelte store with debounced writes (2s)
- `packages/stores/src/fileWatcher.ts` - External change tracking store
- `packages/stores/src/toast.ts` - Toast queue with auto-dismiss
- `packages/stores/src/shortcuts.ts` - Shortcut registry with platform detection
- `apps/desktop/src/lib/recovery.ts` - Recovery client
- `apps/desktop/src/lib/fileWatcher.ts` - File watcher client
- `apps/desktop/src/lib/errorReporter.ts` - Error reporter client
- `apps/desktop/src/lib/components/RecoveryDialog.svelte` - Recovery UI
- `apps/desktop/src/lib/components/ExternalChangeDialog.svelte` - External change UI
- `apps/desktop/src/lib/components/Toast.svelte` - Individual toast component
- `apps/desktop/src/lib/components/ToastContainer.svelte` - Fixed position stack
- Enhanced `SearchDropdown.svelte` with scoring algorithm
- Settings modal with Shortcuts tab

**Electron Reference Files:**
- `electron/services/recoveryManager.ts` (247 lines) - WAL-based recovery
- `electron/services/fileWatcher.ts` (270 lines) - External change detection
- `electron/services/errorReportingService.ts` (242 lines) - Anonymous error reporting
- `src/store/useToastStore.ts` (61 lines) - Toast notification state
- `src/components/ToastContainer.tsx` (2.6K) - Toast UI
- `src/components/RecoveryPrompt.tsx` (2.0K) - Recovery banner UI
- `src/components/SearchBar.tsx` + `SearchDropdown.tsx` (8.3K) - Document search

---

#### Overview

Phase 7 focuses on reliability, error handling, and user experience polish. The key features are:

1. **Recovery Manager** - Write-Ahead Logging (WAL) to recover unsaved changes after crashes
2. **File Watcher** - Detect external file changes and prompt user to reload
3. **Error Reporting** - Anonymous, opt-in error reporting to help improve the app
4. **Toast Notifications** - Non-blocking feedback for user actions
5. **Search** - Fuzzy search across all documents in workspace
6. **Keyboard Shortcuts** - Global and context-specific shortcuts

---

#### 7.1 Rust Backend: Recovery Manager (`src-tauri/src/services/recovery_manager.rs`)

Write-Ahead Logging (WAL) to recover unsaved changes after crashes.

**Types:**
```rust
pub struct RecoveryConfig {
    pub enabled: bool,           // Default: true
    pub wal_interval_ms: u64,    // Default: 500ms
}

pub struct RecoveryFile {
    pub file_key: String,        // Relative path from workspace root
    pub content: String,         // Recovered document content (JSON)
    pub timestamp: DateTime<Utc>, // When recovery was created
}

pub struct RecoveryManager {
    workspace_root: PathBuf,
    recovery_dir: PathBuf,       // .midlight/recovery/
    config: RecoveryConfig,
    active_wals: HashMap<String, WalHandle>,
}

struct WalHandle {
    file_key: String,
    last_content: String,
    last_write: Instant,
}
```

**Methods implemented:**
- [x] `new(workspace_root: PathBuf, config: RecoveryConfig) -> Self`
  - Create recovery directory if not exists
  - Initialize empty active_wals map

- [x] `start_wal(&mut self, file_key: &str, content: &str)`
  - Create WalHandle with initial content
  - Write initial WAL file

- [x] `update_wal(&mut self, file_key: &str, content: &str) -> Result<(), RecoveryError>`
  - Only write if content differs from last_content
  - Update timestamp
  - Debounce writes (only write if 500ms since last write)

- [x] `stop_wal(&mut self, file_key: &str)`
  - Remove from active_wals map
  - Clear WAL file (successful save)

- [x] `stop_all_wal(&mut self)`
  - Stop all WAL timers on app quit

- [x] `check_for_recovery(&self) -> Result<Vec<RecoveryFile>, RecoveryError>`
  - Scan recovery directory for orphaned WAL files
  - Return list of recoverable files

- [x] `apply_recovery(&self, file_key: &str) -> Result<String, RecoveryError>`
  - Read and return WAL content
  - Don't delete yet (let UI confirm)

- [x] `clear_recovery(&self, file_key: &str) -> Result<(), RecoveryError>`
  - Delete WAL file after user accepts or discards

- [x] `has_unique_recovery(&self, file_key: &str, current_content: &str) -> bool`
  - Check if recovery content differs from current file content

**WAL File Format:**
```
.midlight/recovery/{hash_of_file_key}.wal.json
{
  "file_key": "notes/ideas.md",
  "content": "{\"type\":\"doc\",...}",
  "timestamp": "2025-01-08T12:34:56Z"
}
```

**Crate dependencies:**
- `chrono` (timestamps)
- `sha2` (file key hashing for WAL filenames)

---

#### 7.2 Rust Backend: File Watcher (`src-tauri/src/services/file_watcher.rs`)

Detect external file changes using the `notify` crate.

**Types:**
```rust
pub struct FileWatcherConfig {
    pub debounce_ms: u64,        // Default: 500ms
    pub patterns: Vec<String>,   // Default: ["**/*.md"]
    pub ignored: Vec<String>,    // Default: ["node_modules", ".git", ".midlight"]
}

pub enum FileChangeType {
    Modified,
    Created,
    Deleted,
}

pub struct FileChangeEvent {
    pub change_type: FileChangeType,
    pub file_key: String,        // Relative path
    pub absolute_path: PathBuf,
    pub timestamp: DateTime<Utc>,
}

pub struct FileWatcher {
    workspace_root: PathBuf,
    config: FileWatcherConfig,
    watcher: Option<RecommendedWatcher>,
    file_mtimes: HashMap<String, SystemTime>,
    saving_files: HashSet<String>, // Files currently being saved by app
    event_tx: mpsc::Sender<FileChangeEvent>,
}
```

**Methods implemented:**
- [x] `new(workspace_root: PathBuf, config: FileWatcherConfig) -> (Self, mpsc::Receiver<FileChangeEvent>)`
  - Create watcher instance
  - Return event receiver for main thread

- [x] `start(&mut self) -> Result<(), WatcherError>`
  - Initialize notify watcher
  - Start watching workspace directory
  - Configure ignored paths

- [x] `stop(&mut self) -> Result<(), WatcherError>`
  - Stop watcher and clean up

- [x] `mark_saving(&mut self, file_key: &str)`
  - Add to saving_files set
  - Prevents emitting change event for app's own saves

- [x] `clear_saving(&mut self, file_key: &str)`
  - Remove from saving_files set
  - Update mtime cache

- [x] `update_mtime(&mut self, file_key: &str) -> Result<(), WatcherError>`
  - Update cached modification time

- [x] `has_external_change(&self, file_key: &str) -> bool`
  - Compare current mtime with cached

- [x] `get_watched_files(&self) -> Vec<String>`
  - List all currently watched files

**Event Handling Logic:**
1. Receive raw notify event
2. Check if file matches patterns and not ignored
3. Check if file is in saving_files set (skip if so)
4. Debounce: wait 500ms, check if mtime still differs
5. Emit FileChangeEvent to channel

**Crate dependencies:**
- `notify = "6"` (file system notifications)
- `glob` (pattern matching)

---

#### 7.3 Rust Backend: Error Reporting (`src-tauri/src/services/error_reporting.rs`)

Anonymous, opt-in error reporting service.

**Types:**
```rust
pub enum ErrorCategory {
    Update,
    Import,
    Export,
    FileSystem,
    Crash,
    Uncaught,
}

pub struct ErrorReport {
    pub category: ErrorCategory,
    pub error_type: String,      // e.g., "checksum", "network", "permission"
    pub message: String,         // Sanitized message
    pub app_version: String,
    pub platform: String,        // "macos", "windows", "linux"
    pub arch: String,            // "x64", "arm64"
    pub os_version: String,
    pub context: Option<HashMap<String, serde_json::Value>>,
    pub timestamp: DateTime<Utc>,
    pub session_id: String,      // Anonymous, regenerated per launch
}

pub struct ErrorReportingService {
    enabled: bool,
    session_id: String,
    endpoint: String,            // https://midlight.ai/api/error-report
}
```

**Methods implemented:**
- [x] `new() -> Self`
  - Generate random session_id (UUID v4)
  - Load enabled state from settings

- [x] `set_enabled(&mut self, enabled: bool)`
  - Toggle error reporting

- [x] `is_enabled(&self) -> bool`

- [x] `report_error(&self, category: ErrorCategory, error_type: &str, message: &str, context: Option<HashMap<String, Value>>)`
  - Sanitize message (remove PII)
  - Build ErrorReport
  - Fire-and-forget HTTP POST (don't await)

- [x] `report_update_error(&self, error_type: &str, message: &str)`
  - Convenience wrapper for update errors

- [x] `report_import_error(&self, error_type: &str, message: &str)`
  - Convenience wrapper for import errors

- [x] `sanitize_message(message: &str) -> String`
  - Replace Unix paths: `/Users/*/` → `/Users/[REDACTED]/`
  - Replace Windows paths: `C:\Users\*\` → `C:\Users\[REDACTED]\`
  - Replace emails: `*@*.com` → `[EMAIL_REDACTED]`
  - Truncate to 1000 chars (+ UUIDs, IPs, bearer tokens)

**HTTP Request:**
```
POST https://midlight.ai/api/error-report
Content-Type: application/json

{
  "category": "import",
  "error_type": "parse_error",
  "message": "Failed to parse YAML front matter",
  "app_version": "1.2.3",
  "platform": "macos",
  "arch": "arm64",
  "os_version": "14.2.1",
  "context": { "file_type": "obsidian" },
  "timestamp": "2025-01-08T12:34:56Z",
  "session_id": "a1b2c3d4-e5f6-..."
}
```

**Crate dependencies:**
- `reqwest` (HTTP client, already included)
- `regex` (PII sanitization)

---

#### 7.4 Tauri Commands (`src-tauri/src/commands/recovery.rs`)

IPC handlers for recovery and file watcher operations.

**Recovery Commands:**
- [x] `recovery_check(workspace_root: String) -> Result<Vec<RecoveryFile>, String>`
  - Check for orphaned WAL files on workspace open

- [x] `recovery_apply(workspace_root: String, file_key: String) -> Result<String, String>`
  - Return recovered content for file

- [x] `recovery_discard(workspace_root: String, file_key: String) -> Result<(), String>`
  - Delete WAL file (user chose to discard)

- [x] `recovery_clear(workspace_root: String, file_key: String) -> Result<(), String>`
  - Delete WAL file (user accepted recovery and saved)

**File Watcher Commands:**
- [x] `watcher_start(workspace_root: String) -> Result<(), String>`
  - Start watching workspace

- [x] `watcher_stop(workspace_root: String) -> Result<(), String>`
  - Stop watching workspace

- [x] `watcher_mark_saving(workspace_root: String, file_key: String) -> Result<(), String>`
  - Mark file as being saved by app

- [x] `watcher_clear_saving(workspace_root: String, file_key: String) -> Result<(), String>`
  - Clear saving flag after save completes

**Error Reporting Commands:**
- [x] `error_reporting_set_enabled(enabled: bool) -> Result<(), String>`
  - Toggle error reporting

- [x] `error_reporting_is_enabled() -> Result<bool, String>`
  - Check if enabled

- [x] `error_report(category: String, error_type: String, message: String, context: Option<Value>) -> Result<(), String>`
  - Report error from frontend

**Event Emissions:**
- [x] `file:external-change` - Emit when external file change detected
  - Payload: `{ fileKey: string, changeType: string }`

---

#### 7.5 Svelte Stores: Toast & Recovery (`packages/stores/src/`)

**Toast Store (`toast.ts`):**
```typescript
export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration: number;  // Default: 5000ms
}

export interface ToastState {
  toasts: Toast[];
}

// Store methods
- addToast(type: ToastType, message: string, duration?: number): void
- removeToast(id: string): void
- clearAll(): void

// Convenience API
export const toast = {
  success: (message: string, duration?: number) => void,
  error: (message: string, duration?: number) => void,
  info: (message: string, duration?: number) => void,
  warning: (message: string, duration?: number) => void,
};
```

**Recovery Store (`recovery.ts`):**
```typescript
export interface RecoveryFile {
  fileKey: string;
  content: string;
  timestamp: Date;
}

export interface RecoveryState {
  hasRecovery: boolean;
  recoveryFiles: RecoveryFile[];
  currentRecovery: RecoveryFile | null;  // For the active file
}

// Store methods
- setRecoveryFiles(files: RecoveryFile[]): void
- setCurrentRecovery(file: RecoveryFile | null): void
- clearRecovery(): void
```

---

#### 7.6 Svelte UI: Toast Container (`packages/ui/src/components/ToastContainer.svelte`)

Toast notification display component.

**Features:**
- [x] Fixed position bottom-right
- [x] Slide-in/out animations (150ms)
- [x] Auto-dismiss after duration
- [x] Manual close button
- [x] Color-coded by type with icons:
  - success: Green checkmark
  - error: Red warning
  - warning: Yellow alert
  - info: Blue information
- [x] Stacking with 8px gap
- [x] Max width 320px
- [x] Backdrop blur effect
- [x] Dark mode support

**Animation:**
```css
.toast-enter { transform: translateX(100%); opacity: 0; }
.toast-enter-active { transform: translateX(0); opacity: 1; transition: all 150ms; }
.toast-exit { transform: translateX(0); opacity: 1; }
.toast-exit-active { transform: translateX(100%); opacity: 0; transition: all 150ms; }
```

---

#### 7.7 Svelte UI: Recovery Prompt (`apps/desktop/src/lib/components/RecoveryPrompt.svelte`)

Banner shown when recovered content is available.

**Props:**
```typescript
interface Props {
  filePath: string;
  recoveryTime: Date | null;
  onRecover: () => void;
  onDiscard: () => void;
}
```

**Features:**
- [x] Yellow warning banner at top of editor
- [x] Alert icon
- [x] File name display
- [x] Relative time ("2 hours ago", "just now")
- [x] "Restore changes" button (primary action)
- [x] "Discard" button (secondary action)
- [x] Dark mode support

**Time Formatting Helper:**
```typescript
function formatTimeAgo(date: Date): string {
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)} minutes ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} hours ago`;
  return `${Math.floor(seconds / 86400)} days ago`;
}
```

---

#### 7.8 Svelte UI: Search (`apps/desktop/src/lib/components/SearchBar.svelte` + `SearchDropdown.svelte`)

Global document search with fuzzy matching.

**SearchBar Features:**
- [x] Cmd/Ctrl+K to open
- [x] Search input with icon
- [x] Escape to close
- [x] Placeholder: "Search documents..."

**SearchDropdown Features:**
- [x] Fuzzy search by filename (case-insensitive)
- [x] Real-time results (max 15 files)
- [x] Keyboard navigation (Arrow Up/Down/Enter)
- [x] Selected item highlighting
- [x] File icons by type
- [x] Folder path display for context
- [x] "No files found" state
- [x] Auto-scroll selected into view
- [x] Keyboard hints footer

**Search Algorithm:**
```typescript
const filteredFiles = allFiles
  .filter(file => file.name.toLowerCase().includes(query.toLowerCase()))
  .slice(0, 15);
```

**File Categories & Icons:**
```typescript
type FileCategory = 'native' | 'compatible' | 'viewable' | 'other';

const iconMap = {
  native: FileTextIcon,      // .md, .midlight
  compatible: FileTextIcon,  // .txt
  viewable: ImageIcon,       // .png, .jpg
  other: FileIcon,           // everything else
};
```

---

#### 7.9 Svelte UI: External Change Dialog (`apps/desktop/src/lib/components/ExternalChangeDialog.svelte`)

Dialog shown when file is modified externally.

**Props:**
```typescript
interface Props {
  open: boolean;
  fileKey: string;
  onReload: () => void;
  onKeepLocal: () => void;
  onClose: () => void;
}
```

**Features:**
- [x] Modal dialog with backdrop
- [x] Warning icon
- [x] File name display
- [x] "Reload from disk" button (loads external version)
- [x] "Keep my version" button (overwrites external)
- [x] "Close" to dismiss without action
- [x] Warning about data loss for each option

---

#### 7.10 Keyboard Shortcuts

**Global Shortcuts (via Tauri GlobalShortcut or window listener):**

| Shortcut | Action | Context |
|----------|--------|---------|
| Cmd/Ctrl+K | Open search | Global |
| Cmd/Ctrl+S | Save document | Editor focused |
| Cmd/Ctrl+N | New document | Global |
| Cmd/Ctrl+O | Open workspace | Global |
| Cmd/Ctrl+W | Close tab | Editor focused |
| Cmd/Ctrl+Shift+P | Export to PDF | Editor focused |
| Cmd/Ctrl+, | Open settings | Global |
| Cmd/Ctrl+/ | Toggle AI panel | Global |

**Editor Shortcuts (handled by Tiptap):**

| Shortcut | Action |
|----------|--------|
| Cmd/Ctrl+B | Bold |
| Cmd/Ctrl+I | Italic |
| Cmd/Ctrl+U | Underline |
| Cmd/Ctrl+Shift+S | Strikethrough |
| Cmd/Ctrl+Z | Undo |
| Cmd/Ctrl+Shift+Z | Redo |

**Implementation:**
```typescript
// Global shortcut registration
onMount(() => {
  const handleKeyDown = (e: KeyboardEvent) => {
    const isMod = e.metaKey || e.ctrlKey;

    if (isMod && e.key === 'k') {
      e.preventDefault();
      openSearch();
    }
    // ... more shortcuts
  };

  window.addEventListener('keydown', handleKeyDown);
  return () => window.removeEventListener('keydown', handleKeyDown);
});
```

---

#### 7.11 Integration Points

**Editor Integration:**
- [x] Start WAL on file open
- [x] Update WAL on content change (debounced)
- [x] Stop WAL on file save
- [x] Show RecoveryPrompt when hasRecovery is true
- [x] Handle recovery accept/discard

**Workspace Integration:**
- [x] Check for recovery on workspace open
- [x] Start file watcher on workspace open
- [x] Stop file watcher on workspace close
- [x] Handle external change events

**App Integration:**
- [x] Register global keyboard shortcuts on mount
- [x] Show ToastContainer in root layout
- [x] Error boundary reports errors to error service

---

#### Success Criteria

- [x] Unsaved changes recovered after crash
- [x] External file changes detected and user prompted
- [x] Errors reported anonymously (opt-in)
- [x] Toast notifications for all user actions
- [x] Cmd/Ctrl+K opens document search
- [x] All standard keyboard shortcuts work
- [ ] App performs well with large workspaces (1000+ files) - needs testing

---

#### Testing Strategy

**Unit Tests:**
- RecoveryManager WAL write/read/clear
- FileWatcher event debouncing
- Error message sanitization
- Time formatting helpers

**Integration Tests:**
- Recovery flow: crash simulation → restart → recovery prompt
- File watcher: external edit → dialog → reload/keep
- Search: type query → navigate results → open file

**E2E Tests:**
- Full recovery workflow
- External change handling
- Keyboard shortcuts

---

### Phase 8: Desktop Polish (P2)

**Goal:** Desktop-specific features and packaging

#### Tasks
1. Implement auto-updater via Tauri plugin
2. Add native menus (macOS menu bar, Windows overlay)
3. Configure app signing and notarization
4. Build installers for all platforms
5. Set up release pipeline

#### Success Criteria
- [ ] App auto-updates on all platforms
- [ ] Native menus work correctly
- [ ] App signed and notarized for macOS
- [ ] Installers work for Windows/macOS/Linux

---

### Phase 9: Web-Specific Features (P1) - Weeks 20-22

**Goal:** Web app ready for production

#### Tasks
1. Optimize OPFS storage adapter
2. Implement IndexedDB fallback
3. Add cloud sync backend endpoints
4. Build sync status UI
5. Add offline indicator
6. Implement service worker for offline
7. Performance optimization for web

#### Success Criteria
- [ ] Web editor works offline
- [ ] Documents sync to cloud (optional)
- [ ] Sync conflicts handled gracefully
- [ ] Performance acceptable on mobile

---

## Detailed Task Breakdown

### Tiptap Extensions Migration

Each extension needs to be ported from the React implementation. Since Tiptap is framework-agnostic, most extensions work as-is.

| Extension | Source File | Complexity | Notes |
|-----------|-------------|------------|-------|
| AIAnnotation | `extensions/AIAnnotation.ts` | Medium | Mark with data attributes |
| FontSize | `extensions/FontSize.ts` | Low | TextStyle attribute |
| DiffAdded | `extensions/DiffMark.ts` | Low | Simple mark |
| DiffRemoved | `extensions/DiffMark.ts` | Low | Simple mark with strikethrough |
| PageSplitting | `extensions/PageSplitting.ts` | High | Page break calculation |
| ResizableImage | `extensions/ResizableImage.ts` | High | NodeView with drag handles |
| TextColor | `extensions/TextColor.ts` | Low | TextStyle attribute |
| TextHighlight | `extensions/TextHighlight.ts` | Low | Mark with color |
| Underline | `extensions/Underline.ts` | Low | Simple mark |
| ClickableHorizontalRule | `extensions/ClickableHorizontalRule.ts` | Low | Custom node |

### Rust Services Implementation

#### ImageManager (New)

```rust
pub struct ImageManager {
    images_dir: PathBuf,
}

impl ImageManager {
    pub async fn store_image(&self, data_url: &str, original_name: Option<&str>) -> Result<String>;
    pub async fn get_image_data_url(&self, ref_id: &str) -> Result<String>;
    pub async fn exists(&self, ref_id: &str) -> bool;
    pub async fn delete(&self, ref_id: &str) -> Result<()>;
    pub async fn gc(&self, referenced_refs: &HashSet<String>) -> Result<u32>;
}
```

#### RecoveryManager (New)

```rust
pub struct RecoveryManager {
    recovery_dir: PathBuf,
    wal_interval_ms: u64,
}

impl RecoveryManager {
    pub async fn start_wal(&self, file_key: &str, content: &str) -> Result<()>;
    pub async fn update_wal(&self, file_key: &str, content: &str) -> Result<()>;
    pub async fn stop_wal(&self, file_key: &str) -> Result<()>;
    pub async fn check_for_recovery(&self) -> Result<Vec<RecoveryInfo>>;
    pub async fn get_recovery_content(&self, file_key: &str) -> Result<String>;
    pub async fn discard_recovery(&self, file_key: &str) -> Result<()>;
}
```

#### FileWatcher (New)

```rust
use notify::{Watcher, RecursiveMode};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    debounce_ms: u64,
}

impl FileWatcher {
    pub fn start(&mut self, root: &Path) -> Result<()>;
    pub fn stop(&mut self) -> Result<()>;
    pub fn mark_saving(&self, file_key: &str);
    pub fn clear_saving(&self, file_key: &str);
    pub fn has_external_change(&self, file_key: &str) -> bool;
}
```

### Web Storage Adapter Enhancements

```typescript
class WebStorageAdapter implements StorageAdapter {
  // OPFS for file content
  private opfsRoot: FileSystemDirectoryHandle;

  // IndexedDB for metadata and checkpoints
  private db: IDBDatabase;

  async init(workspaceId: string): Promise<void>;

  // File operations via OPFS
  async readDir(path: string): Promise<FileEntry[]>;
  async readFile(path: string): Promise<string>;
  async writeFile(path: string, content: string): Promise<void>;

  // Checkpoint storage via IndexedDB
  async getCheckpoints(filePath: string): Promise<Checkpoint[]>;
  async createCheckpoint(filePath: string, content: string): Promise<Checkpoint>;

  // Image storage via OPFS
  async storeImage(dataUrl: string): Promise<string>;
  async getImageDataUrl(ref: string): Promise<string>;

  // Recovery via IndexedDB
  async saveRecovery(filePath: string, content: string): Promise<void>;
  async getRecovery(filePath: string): Promise<string | null>;
}
```

---

## Testing Strategy

### Unit Tests

| Package | Framework | Coverage Target |
|---------|-----------|-----------------|
| @midlight/core | Vitest | 90% |
| @midlight/stores | Vitest | 80% |
| @midlight/ui | Vitest + Testing Library | 70% |
| Rust services | cargo test | 85% |

### Integration Tests

| Feature | Test Type | Framework |
|---------|-----------|-----------|
| Document serialization round-trip | Unit | Vitest |
| Tauri IPC commands | Integration | Tauri test utils |
| OPFS storage | Integration | Playwright |
| Auth flow | E2E | Playwright |
| AI chat | E2E | Playwright + MSW |

### E2E Tests

| Scenario | Platform | Framework |
|----------|----------|-----------|
| Create and edit document | Desktop | WebdriverIO |
| Create and edit document | Web | Playwright |
| Version history workflow | Both | Playwright |
| AI agent workflow | Both | Playwright |
| Import/export | Desktop | WebdriverIO |

---

## Risk Mitigation

### Technical Risks

| Risk | Mitigation |
|------|------------|
| OPFS browser support | IndexedDB fallback, feature detection |
| Tauri 2.0 stability | Pin versions, test across platforms |
| LLM API changes | Abstract behind service layer |
| Large file performance | Virtualized lists, pagination |
| Sync conflicts | Last-write-wins initially, conflict UI later |

### Schedule Risks

| Risk | Mitigation |
|------|------------|
| Tiptap extension complexity | Prioritize core extensions first |
| AI agent edge cases | Comprehensive test suite |
| Cross-platform issues | CI testing on all platforms |
| Scope creep | Feature freeze before each phase |

### Rollback Plan

1. Electron app maintained in parallel during migration
2. Feature flags for gradual rollout
3. Database migrations are backward-compatible
4. Users can export data at any time

---

## Appendix: Full IPC Command Reference

See the [Electron preload.ts](../ai-doc-app/electron/preload.ts) for the complete API surface that needs to be replicated.

## Appendix: Electron Service Reference

See the [Electron services directory](../ai-doc-app/electron/services/) for detailed implementation reference.

---

*Last updated: January 8, 2025*
*Document version: 1.4*
