# Midlight Migration Plan: Electron → Tauri + Web

**Goal:** Achieve full feature parity with the existing Electron app while supporting both desktop (Tauri) and web (midlight.ai/editor) platforms.

**Current Status:** Phase 5 (Authentication & Subscription) ~80% complete. Stripe integration and quota UI remaining.

**Latest Session (January 2025):** Completed Phase 4 AI annotations. Audited Phase 5 - discovered auth (email/password, Google OAuth), token management, and UI gating already implemented. Remaining: Stripe payment integration, quota enforcement UI.

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
| AuthModal UI | ✅ | Login/signup modes, Google OAuth button |
| Chat Auth Gating | ✅ | Sign-in required prompt in ChatPanel |
| Settings Auth Section | ✅ | Account display, sign out, subscription tier |

### In Progress 🔄

| Component | Status | Remaining Work |
|-----------|--------|----------------|
| Document serialization integration | 🔄 | TypeScript serializers exist, need frontend integration |
| Quota tracking | 🔄 | Backend fetching works, need client-side UI/enforcement |
| Subscription management | 🔄 | Tier display works, need Stripe checkout/upgrade flow |

### Not Started ❌

| Component | Priority |
|-----------|----------|
| Recovery manager | P1 |
| File watcher | P1 |
| Import service (Obsidian/Notion) | P2 |
| Auth service | P1 |
| Subscription service | P2 |
| Auto-updater | P2 |
| Error reporting | P3 |

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
- [ ] `importDocx()` - DOCX import
- [ ] `importDocxFromPath(filePath)` - DOCX from path
- [ ] `exportPdf()` - PDF export
- [ ] `exportDocx(content)` - DOCX export

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

#### Subscription Operations (4)
- [ ] `subscription.getStatus()` - Get subscription
- [ ] `subscription.createCheckout(priceType, urls)` - Stripe checkout
- [ ] `subscription.createPortal(returnUrl)` - Customer portal
- [ ] `subscription.getPrices()` - Get pricing

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
| SubscriptionService | TS | TS (shared) | TS (shared) | ❌ |
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
4. ⏸️ Implement file watcher (Tauri) for external changes - Deferred to Phase 7
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

### Phase 5: Authentication & Subscription (P1) - ~80% COMPLETE

**Goal:** User accounts with subscription management

**Status:** ✅ Core auth complete. Stripe payment integration and quota enforcement remaining.

#### Tasks
1. ✅ Build AuthService with JWT/refresh tokens (Rust: `auth_service.rs` 648 lines)
2. ✅ Implement email/password login/signup (`auth_login`, `auth_signup` commands)
3. ✅ Add Google OAuth flow (local TCP callback server + event-driven)
4. ✅ Build AuthModal UI (`AuthModal.svelte` with login/signup modes)
5. ❌ Connect SubscriptionService to Stripe (not started)
6. ❌ Build UpgradeModal with pricing (not started)
7. 🔄 Add quota tracking and limits (fetching works, enforcement missing)

#### Implementation Notes
- **Auth Store** (`packages/stores/src/auth.ts`):
  - User, Subscription, Quota types
  - `isAuthenticated`, `isInitializing` state
  - `setUser()`, `setSubscription()`, `logout()` methods
- **Auth Client** (`apps/desktop/src/lib/auth.ts`):
  - `init()` - Silent refresh on app start
  - `login(email, password)`, `signup()`, `loginWithGoogle()`
  - `getAccessToken()` - Used by LLM client
  - Event listeners for OAuth completion
- **Rust AuthService** (`src-tauri/services/auth_service.rs`):
  - In-memory access token (never persisted)
  - Refresh token in httpOnly cookies
  - 60-second early refresh buffer
  - OAuth code exchange
- **Tauri Commands** (`src-tauri/commands/auth.rs`):
  - 12 commands: init, signup, login, logout, login_with_google, handle_oauth_callback, get_user, get_subscription, get_quota, is_authenticated, get_state, get_access_token
- **UI Integration**:
  - AuthModal in App.svelte
  - Account section in SettingsModal
  - Sign-in gate in ChatPanel

#### Remaining Work
1. **Stripe Integration** - Build upgrade flow, connect to backend checkout endpoints
2. **Quota Enforcement** - Display quota in UI, prevent requests when exceeded
3. **Password Reset** - Add forgot password flow
4. **Account Management** - Change email/password, delete account

#### Success Criteria
- [x] Users can sign up and login
- [x] Google OAuth works
- [x] Subscription status reflected in UI
- [ ] Quota limits enforced (backend enforces, client needs UI)
- [ ] Upgrade flow works end-to-end (needs Stripe)

---

### Phase 6: Import/Export (P2) - Weeks 14-15

**Goal:** Import from Obsidian/Notion, export to PDF/DOCX

#### Tasks
1. Build ImportService in Rust
2. Implement Obsidian vault import with wiki-link conversion
3. Implement Notion export import
4. Build ImportWizard UI
5. Add DOCX import via mammoth.js
6. Add PDF export via print API
7. Add DOCX export via docx.js

#### Success Criteria
- [ ] Obsidian vaults import with formatting preserved
- [ ] Notion exports import correctly
- [ ] DOCX files can be imported
- [ ] Documents export to PDF
- [ ] Documents export to DOCX

---

### Phase 7: Recovery & Polish (P1) - Weeks 16-17

**Goal:** Crash recovery, error handling, polish

#### Tasks
1. Build RecoveryManager with WAL
2. Add crash recovery prompt
3. Implement error reporting service
4. Build Toast notifications
5. Add keyboard shortcuts
6. Implement search across documents
7. Performance optimization

#### Success Criteria
- [ ] Unsaved changes recovered after crash
- [ ] Errors reported anonymously (opt-in)
- [ ] All actions have keyboard shortcuts
- [ ] Search finds content across files
- [ ] App performs well with large workspaces

---

### Phase 8: Desktop Polish (P2) - Weeks 18-19

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

*Last updated: January 6, 2025*
*Document version: 1.3*
