# Midlight-Next Tauri/Rust Application Audit

**Audit Date:** January 9, 2026
**Codebase Version:** 0.1.0
**Auditor:** Claude Code

---

## Executive Summary

Midlight-Next is a Tauri 2.x desktop application built as the successor to the Electron-based Midlight editor. The application is a local-first, AI-native document editor with comprehensive import/export capabilities, version control, and LLM integration.

| Metric | Value |
|--------|-------|
| **Total Rust Code** | ~12,944 lines across 35 files |
| **IPC Commands** | 75+ commands across 11 categories |
| **Services** | 18 backend services |
| **Framework** | Tauri 2.x + Svelte 5 + SvelteKit |
| **Build System** | Turborepo + pnpm workspaces |

### Overall Assessment

| Area | Rating | Notes |
|------|--------|-------|
| Architecture | **Strong** | Clean service-oriented design with workspace isolation |
| Security | **Good** | Solid path validation and input sanitization; some gaps |
| Code Quality | **Good** | Consistent patterns, proper error handling |
| Testing | **Needs Improvement** | Limited test coverage, especially for services |
| Documentation | **Adequate** | Migration plan exists; inline docs sparse |
| Performance | **Good** | Effective use of deduplication, compression, streaming |

---

## Table of Contents

1. [Project Structure](#1-project-structure)
2. [Architecture Overview](#2-architecture-overview)
3. [Rust Backend Analysis](#3-rust-backend-analysis)
4. [Tauri Configuration](#4-tauri-configuration)
5. [IPC Commands Reference](#5-ipc-commands-reference)
6. [State Management](#6-state-management)
7. [Security Analysis](#7-security-analysis)
8. [Dependencies Audit](#8-dependencies-audit)
9. [Testing Assessment](#9-testing-assessment)
10. [Performance Considerations](#10-performance-considerations)
11. [Recommendations](#11-recommendations)

---

## 1. Project Structure

### Monorepo Layout

```
midlight-next/
├── apps/
│   ├── desktop/          # Tauri desktop application
│   │   ├── src/          # Svelte 5 frontend
│   │   ├── src-tauri/    # Rust backend
│   │   └── dist/         # Built frontend assets
│   └── web/              # SvelteKit web application
├── packages/
│   ├── core/             # Shared TypeScript (types, serialization, utils)
│   ├── stores/           # Svelte state management
│   └── ui/               # Reusable Svelte components + Tiptap extensions
├── scripts/              # Build and versioning scripts
├── docs/                 # Project documentation
├── package.json          # Monorepo root config
├── pnpm-workspace.yaml   # Workspace definition
├── turbo.json            # Turborepo build orchestration
└── vitest.config.ts      # Root test configuration
```

### Code Statistics

| Category | Files | Lines of Code |
|----------|-------|---------------|
| Rust Backend | 35 | ~12,944 |
| Commands | 15 | ~2,400 |
| Services | 18 | ~10,500 |
| Largest File | import_service.rs | 1,333 |

---

## 2. Architecture Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Desktop Application                       │
├─────────────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────────────────┐ │
│  │                  Svelte 5 Frontend                      │ │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌───────────┐ │ │
│  │  │ Editor  │  │ Sidebar │  │ AI Chat │  │ Versions  │ │ │
│  │  └────┬────┘  └────┬────┘  └────┬────┘  └─────┬─────┘ │ │
│  └───────┼────────────┼───────────┼──────────────┼───────┘ │
│          │            │           │              │          │
│          └────────────┴─────┬─────┴──────────────┘          │
│                             │                                │
│                    ┌────────▼────────┐                      │
│                    │  Tauri Bridge   │                      │
│                    │  (IPC/Events)   │                      │
│                    └────────┬────────┘                      │
├─────────────────────────────┼───────────────────────────────┤
│  ┌──────────────────────────▼──────────────────────────────┐│
│  │                    Rust Backend                          ││
│  │  ┌─────────────────────────────────────────────────────┐││
│  │  │                  Commands Layer                      │││
│  │  │  fs │ workspace │ versions │ llm │ auth │ import    │││
│  │  └─────────────────────────┬───────────────────────────┘││
│  │                            │                             ││
│  │  ┌─────────────────────────▼───────────────────────────┐││
│  │  │                  Services Layer                      │││
│  │  │  WorkspaceManager │ CheckpointManager │ AuthService │││
│  │  │  LLMService │ ImportService │ RecoveryManager       │││
│  │  └─────────────────────────┬───────────────────────────┘││
│  │                            │                             ││
│  │  ┌─────────────────────────▼───────────────────────────┐││
│  │  │                  Storage Layer                       │││
│  │  │  ObjectStore (SHA-256) │ ImageManager │ FileWatcher │││
│  │  └─────────────────────────────────────────────────────┘││
│  └──────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │  midlight.ai    │
                    │  Backend API    │
                    │  (LLM Proxy)    │
                    └────────┬────────┘
                             │
         ┌───────────────────┼───────────────────┐
         ▼                   ▼                   ▼
    ┌─────────┐        ┌──────────┐       ┌─────────┐
    │ OpenAI  │        │ Anthropic│       │ Gemini  │
    └─────────┘        └──────────┘       └─────────┘
```

### Key Design Patterns

1. **Workspace Isolation**: Each workspace operates independently with its own managers
2. **Content-Addressable Storage**: SHA-256 hashing for deduplication
3. **Command-Service Pattern**: Commands delegate to services for business logic
4. **Event-Based Communication**: Streaming and async operations use Tauri events
5. **Transactional Operations**: Import operations support rollback on failure

---

## 3. Rust Backend Analysis

### Entry Point (`main.rs` / `lib.rs`)

```rust
// main.rs - 6 LOC
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    midlight_lib::run()
}
```

The `lib.rs` (246 LOC) handles:
- Tracing/logging initialization
- Plugin registration (6 plugins)
- Command registration (75+ commands)
- Application state setup
- Native menu and system tray configuration

### Services Overview

| Service | LOC | Responsibility |
|---------|-----|----------------|
| `workspace_manager.rs` | 718 | Central coordinator for workspace operations |
| `checkpoint_manager.rs` | 384 | Git-like version history management |
| `object_store.rs` | 196 | Content-addressable storage (SHA-256) |
| `recovery_manager.rs` | 386 | Write-Ahead Log crash recovery |
| `auth_service.rs` | 871 | OAuth, token management, subscriptions |
| `llm_service.rs` | 640 | HTTP client for LLM API with streaming |
| `agent_executor.rs` | 1,156 | AI agent tool execution (7 tools) |
| `import_service.rs` | 1,333 | Obsidian/Notion import with progress tracking |
| `import_transaction.rs` | 474 | Transactional import with rollback |
| `import_security.rs` | 614 | Path sanitization, filename validation |
| `docx_import.rs` | 1,045 | Word document parsing (ZIP→XML→Tiptap) |
| `docx_export.rs` | 889 | Tiptap JSON→DOCX generation |
| `file_watcher.rs` | 381 | File system monitoring with debouncing |
| `image_manager.rs` | 181 | Image deduplication and reference tracking |
| `error_reporter.rs` | 379 | Anonymous error reporting to backend |
| `error.rs` | 107 | Error type definitions (thiserror) |

### Data Flow Example: Saving a Document

```
Frontend                    Commands                   Services
   │                           │                          │
   │  invoke('workspace_      │                          │
   │   save_document', ...)   │                          │
   │─────────────────────────▶│                          │
   │                          │  WorkspaceManager::      │
   │                          │   save_document()        │
   │                          │─────────────────────────▶│
   │                          │                          │  1. Validate content
   │                          │                          │  2. Hash content (SHA-256)
   │                          │                          │  3. Store in ObjectStore
   │                          │                          │  4. Update RecoveryManager
   │                          │                          │  5. Create checkpoint
   │                          │◀─────────────────────────│
   │◀─────────────────────────│  SaveResult              │
   │                          │                          │
```

---

## 4. Tauri Configuration

### `tauri.conf.json` Analysis

```json
{
  "productName": "Midlight",
  "version": "0.1.0",
  "identifier": "ai.midlight.desktop",
  "build": {
    "devUrl": "http://localhost:1420",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [{
      "width": 1200,
      "height": 800,
      "minWidth": 800,
      "minHeight": 600,
      "titleBarStyle": "Overlay"
    }],
    "security": {
      "csp": "default-src 'self' ipc: tauri:; script-src 'self' 'wasm-unsafe-eval'; ..."  // ✅ CSP enabled
    }
  }
}
```

### Plugins Enabled

| Plugin | Version | Purpose |
|--------|---------|---------|
| `tauri-plugin-shell` | v2 | Execute shell commands |
| `tauri-plugin-dialog` | v2 | Native file dialogs |
| `tauri-plugin-fs` | v2 | File system operations |
| `tauri-plugin-store` | v2 | Persistent key-value storage |
| `tauri-plugin-updater` | v2 | App auto-updates |
| `tauri-plugin-clipboard-manager` | v2 | Clipboard access |

### Auto-Updater Configuration

- **Endpoint**: `https://midlight.ai/releases/tauri-latest.json`
- **Public key**: Configured for signature verification
- **Update flow**: Check → Download → Verify signature → Install

### Release Build Optimizations

```toml
[profile.release]
panic = "abort"
codegen-units = 1
lto = true
opt-level = "s"  # Size optimized
strip = true
```

---

## 5. IPC Commands Reference

### File System Commands (14)

| Command | Parameters | Returns |
|---------|------------|---------|
| `get_default_workspace` | — | `String` |
| `read_dir` | `path: String` | `Vec<FileNode>` |
| `read_file` | `path: String` | `String` |
| `write_file` | `path: String, content: String` | `()` |
| `delete_file` | `path: String` | `()` |
| `rename_file` | `old_path: String, new_path: String` | `()` |
| `file_exists` | `path: String` | `bool` |
| `create_folder` | `path: String` | `()` |
| `create_midlight_file` | `path: String` | `()` |
| `create_new_folder` | `path: String` | `()` |
| `file_duplicate` | `path: String` | `String` |
| `file_trash` | `path: String` | `()` |
| `file_reveal` | `path: String` | `()` |
| `file_copy_to` | `src: String, dst: String` | `String` |
| `file_move_to` | `src: String, dst: String` | `String` |

### Workspace Commands (4)

| Command | Parameters | Returns |
|---------|------------|---------|
| `workspace_init` | `path: String` | `()` |
| `workspace_load_document` | `file_path: String` | `LoadedDocument` |
| `workspace_save_document` | `path: String, content: String` | `SaveResult` |
| `workspace_get_checkpoints` | `path: String` | `Vec<Checkpoint>` |

### Version Commands (4)

| Command | Parameters | Returns |
|---------|------------|---------|
| `get_checkpoints` | `path: String` | `Vec<Checkpoint>` |
| `restore_checkpoint` | `path: String, id: String` | `LoadedDocument` |
| `create_bookmark` | `path: String, id: String, label: String` | `Checkpoint` |
| `compare_checkpoints` | `path: String, id1: String, id2: String` | `DiffResult` |

### LLM Commands (8)

| Command | Parameters | Returns |
|---------|------------|---------|
| `llm_chat` | `options: ChatOptions, auth_token?: String` | `ChatResponse` |
| `llm_chat_stream` | `options: ChatOptions, auth_token?: String` | `()` (events) |
| `llm_chat_with_tools` | `options: ChatOptions, auth_token?: String` | `ChatResponse` |
| `llm_chat_with_tools_stream` | `options: ChatOptions, auth_token?: String` | `()` (events) |
| `llm_get_models` | `provider: String` | `AvailableModels` |
| `llm_get_quota` | `auth_token: String` | `QuotaInfo` |
| `llm_get_status` | `auth_token?: String` | `LLMStatus` |

### Authentication Commands (17)

| Command | Purpose |
|---------|---------|
| `auth_init` | Initialize auth state |
| `auth_signup` | Email/password registration |
| `auth_login` | Email/password login |
| `auth_logout` | Clear session |
| `auth_login_with_google` | Start OAuth flow |
| `auth_handle_oauth_callback` | Process OAuth callback |
| `auth_get_user` | Get current user |
| `auth_get_subscription` | Get subscription status |
| `auth_get_quota` | Get usage quota |
| `auth_is_authenticated` | Check auth status |
| `auth_get_state` | Get OAuth state |
| `auth_get_access_token` | Get access token |
| `auth_forgot_password` | Initiate password reset |
| `auth_reset_password` | Complete password reset |
| `auth_update_profile` | Update user profile |
| `subscription_get_prices` | Get pricing tiers |
| `subscription_create_checkout` | Create Stripe checkout |
| `subscription_create_portal` | Create Stripe portal |

### Import/Export Commands (11)

| Command | Purpose |
|---------|---------|
| `import_select_folder` | Open folder picker |
| `import_detect_source_type` | Detect Obsidian/Notion/Generic |
| `import_analyze_obsidian` | Analyze Obsidian vault |
| `import_analyze_notion` | Analyze Notion export |
| `import_obsidian` | Execute Obsidian import |
| `import_notion` | Execute Notion import |
| `import_cancel` | Cancel import operation |
| `import_select_docx_file` | Open DOCX picker |
| `import_analyze_docx` | Analyze DOCX file |
| `import_docx_file` | Import DOCX file |
| `export_to_docx` | Export to DOCX |
| `export_pdf` | Export to PDF |
| `export_select_save_path` | Open save dialog |

### Recovery Commands (8)

| Command | Purpose |
|---------|---------|
| `recovery_check` | Get recoverable files |
| `recovery_write_wal` | Write to WAL |
| `recovery_clear_wal` | Clear WAL entry |
| `recovery_has_recovery` | Check if recovery exists |
| `recovery_get_content` | Get recovery content |
| `recovery_discard` | Discard single recovery |
| `recovery_discard_all` | Discard all recovery |
| `recovery_has_unique_content` | Check if content differs |

### System Commands (4)

| Command | Purpose |
|---------|---------|
| `show_in_folder` | Reveal file in Finder/Explorer |
| `open_external` | Open URL in browser |
| `get_app_version` | Get current version |
| `get_platform_info` | Get OS/platform info |

### Update Commands (3)

| Command | Purpose |
|---------|---------|
| `check_for_updates` | Check for new version |
| `download_and_install_update` | Download and install |
| `get_current_version` | Get installed version |

---

## 6. State Management

### Rust State Architecture

```rust
pub struct AppState {
    pub workspace_registry: Arc<RwLock<WorkspaceManagerRegistry>>,
}

pub struct WorkspaceManagerRegistry {
    managers: HashMap<PathBuf, WorkspaceManager>,
}

pub struct WorkspaceManager {
    workspace_path: PathBuf,
    object_store: ObjectStore,
    checkpoint_manager: CheckpointManager,
    image_manager: ImageManager,
    recovery_manager: RecoveryManager,
}
```

### State Containers

| State | Scope | Purpose |
|-------|-------|---------|
| `AppState` | Global | Workspace registry access |
| `RecoveryState` | Global | Crash recovery file tracking |
| `FileWatcherState` | Global | File watcher management |
| `ErrorReporterState` | Global | Error collection and reporting |

### Concurrency Model

- **Primary Pattern**: `Arc<RwLock<T>>` for shared mutable state
- **Async Runtime**: Tokio with full features
- **Thread Safety**: All state containers are Send + Sync
- **Global Services**: `lazy_static` for `AUTH_SERVICE` and `LLM_SERVICE`

---

## 7. Security Analysis

### Implemented Security Measures

#### Path Traversal Prevention (`import_security.rs`)

```rust
pub fn sanitize_relative_path(path: &str) -> Result<PathBuf, ImportError> {
    // Reject paths containing ".."
    // Reject absolute paths
    // Normalize path separators
    // Validate within workspace bounds
}
```

#### Filename Sanitization

- **Blocked Windows Reserved Names**: CON, PRN, AUX, NUL, COM1-9, LPT1-9
- **Removed Characters**: `<>:"/\|?*\0`
- **Unicode Normalization**: NFC form
- **Max Length**: 255 characters

#### File Type Validation

```rust
pub enum AllowedExtension {
    Markdown,   // .md, .markdown
    Image,      // .png, .jpg, .jpeg, .gif, .webp, .svg
    Attachment, // .pdf, .doc, .docx, .xls, .xlsx
    Data,       // .json, .yaml, .csv
}
```

#### Size Limits

| Resource | Limit |
|----------|-------|
| YAML files | 1 MB |
| Content for regex | 10 MB |
| Disk space buffer | 10% |
| Large file threshold | 10 MB |

#### URL Safety

- **Blocked Schemes**: `javascript:`, `data:`, `file://`
- **Validation**: URL parsing before embedding

#### Import Safety Features

- Transactional operations with rollback
- Cancellation token support
- Progress throttling (100ms intervals)
- Access permission tracking

### Security Concerns

| Issue | Severity | Location | Recommendation |
|-------|----------|----------|----------------|
| ~~CSP disabled~~ | ~~**Medium**~~ | ~~`tauri.conf.json`~~ | **FIXED** - CSP enabled |
| No rate limiting | **Low** | Import operations | Add rate limiting for resource-intensive ops |
| Token storage | **Medium** | `tauri-plugin-store` | Consider OS keychain integration |
| OAuth state validation | **Low** | `auth_service.rs` | Strengthen state parameter validation |
| File permissions | **Low** | File operations | Validate file permissions explicitly |

### OWASP Considerations

| Vulnerability | Status | Notes |
|---------------|--------|-------|
| Injection (A03) | **Mitigated** | Path sanitization implemented |
| XSS (A07) | **Mitigated** | CSP enabled; sanitization in place |
| SSRF | **N/A** | No server-side requests from user input |
| Insecure Design (A04) | **Good** | Service isolation, error handling |
| Security Misconfiguration (A05) | **Good** | CSP now enabled |

---

## 8. Dependencies Audit

### Rust Dependencies (Cargo.toml)

#### Framework & Runtime

| Crate | Version | Purpose | Risk |
|-------|---------|---------|------|
| `tauri` | 2.x | Desktop framework | Low - Well maintained |
| `tokio` | 1.x | Async runtime | Low - Standard choice |
| `serde` | 1.x | Serialization | Low - Industry standard |

#### File Operations

| Crate | Version | Purpose | Risk |
|-------|---------|---------|------|
| `walkdir` | 2.x | Directory traversal | Low |
| `zip` | 2.2 | Archive handling | Low - Needs size limits |
| `trash` | 3.x | Safe file deletion | Low |
| `notify` | 6.x | File watching | Low |

#### Cryptography

| Crate | Version | Purpose | Risk |
|-------|---------|---------|------|
| `sha2` | 0.10 | SHA-256 hashing | Low - Pure Rust |

#### Document Processing

| Crate | Version | Purpose | Risk |
|-------|---------|---------|------|
| `docx-rs` | 0.4 | DOCX generation | Medium - Less mature |
| `quick-xml` | 0.37 | XML parsing | Low - Entity protection |

#### HTTP & Networking

| Crate | Version | Purpose | Risk |
|-------|---------|---------|------|
| `reqwest` | 0.12 | HTTP client | Low - Feature-rich |

### Frontend Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| `@tauri-apps/api` | 2.2.0 | Tauri bridge |
| `@tiptap/core` | 2.11.5 | Rich text editor |
| `svelte` | 5.16.0 | UI framework |
| `vite` | 6.0.6 | Build tool |
| `tailwindcss` | 3.4.17 | Styling |

### Dependency Recommendations

1. **Pin Critical Versions**: Consider pinning `tauri`, `tokio`, and `serde` to specific versions
2. **Regular Audits**: Run `cargo audit` regularly
3. **Update Schedule**: Establish monthly dependency review cycle

---

## 9. Testing Assessment

### Current Test Coverage

#### Rust Tests

| File | Test Count | Coverage |
|------|------------|----------|
| `object_store.rs` | 4 | Good |
| Other services | 0 | **Missing** |

**Existing Tests (object_store.rs)**:
```rust
#[tokio::test] async fn test_write_and_read()
#[tokio::test] async fn test_deduplication()
#[tokio::test] async fn test_not_found()
```

#### Frontend Tests

| File | Test Count | Coverage |
|------|------------|----------|
| `shortcuts.test.ts` | Multiple | Keyboard handling |
| `fileWatcher.test.ts` | Multiple | File events |
| `recovery.test.ts` | Multiple | Crash recovery |
| `toast.test.ts` | Multiple | Notifications |

### Test Gaps

| Area | Priority | Recommendation |
|------|----------|----------------|
| `checkpoint_manager.rs` | **High** | Version history is critical |
| `import_service.rs` | **High** | Complex import logic |
| `auth_service.rs` | **High** | Security-sensitive |
| `llm_service.rs` | **Medium** | API interactions |
| `recovery_manager.rs` | **Medium** | Data safety |
| `import_security.rs` | **High** | Security validation |
| `docx_import.rs` | **Medium** | File format parsing |

### Testing Infrastructure

```bash
# Frontend tests
npm test              # vitest watch mode
npm run test:run      # Single run
npm run test:coverage # Coverage report

# Rust tests (if properly configured)
cargo test
```

### Recommended Test Strategy

1. **Unit Tests**: Each service should have isolated tests
2. **Integration Tests**: IPC command end-to-end flows
3. **Property-Based Tests**: For path sanitization and validation
4. **Snapshot Tests**: For document serialization/deserialization
5. **E2E Tests**: Critical user journeys with Playwright

---

## 10. Performance Considerations

### Current Optimizations

| Optimization | Implementation | Impact |
|--------------|----------------|--------|
| Content Deduplication | SHA-256 hashing in ObjectStore | Storage reduction |
| Compression | gzip for version history | ~70% size reduction |
| Lazy Loading | Workspaces loaded on-demand | Faster startup |
| Parallel Processing | rayon for imports (batch: 10) | Faster imports |
| Debouncing | 500ms file watcher debounce | Reduced processing |
| Progress Throttling | 100ms import progress interval | Lower IPC overhead |
| Hash-Based Caching | xxhash for change detection | Fast dirty checks |
| Streaming | HTTP streaming for LLM | Memory efficiency |
| Build Optimization | LTO, single codegen, strip | Smaller binaries |

### Potential Bottlenecks

| Area | Concern | Recommendation |
|------|---------|----------------|
| Large Imports | Memory usage with thousands of files | Implement chunked processing |
| Checkpoint History | Unbounded growth | Enforce retention policy |
| Image Deduplication | Full file read for hashing | Use incremental hashing |
| DOCX Processing | Memory-intensive XML parsing | Stream-based parsing |
| File Watching | Watch event storms | Increase debounce for large folders |

### Memory Management

- **Object Store**: Files stored compressed, loaded on demand
- **LLM Streaming**: Responses streamed, not buffered
- **Import Processing**: Parallel processing with bounded batch size

---

## 11. Recommendations

### Critical (Do Immediately)

| Priority | Issue | Action |
|----------|-------|--------|
| ~~P0~~ | ~~CSP disabled~~ | **FIXED** - CSP now enabled |
| P0 | Limited test coverage | Add tests for security-critical services |
| P0 | Token storage | Migrate to OS keychain for auth tokens |

### High Priority

| Priority | Issue | Action |
|----------|-------|--------|
| P1 | Missing service tests | Add unit tests for all services |
| P1 | Error messages | Review for information leakage |
| P1 | Rate limiting | Add rate limiting for import operations |
| P1 | Dependency audit | Set up automated `cargo audit` in CI |

### Medium Priority

| Priority | Issue | Action |
|----------|-------|--------|
| P2 | OAuth state validation | Strengthen state parameter handling |
| P2 | File permissions | Add explicit permission checks |
| P2 | Documentation | Add inline documentation for services |
| P2 | Integration tests | Add IPC command integration tests |

### Low Priority

| Priority | Issue | Action |
|----------|-------|--------|
| P3 | Code coverage metrics | Set up coverage reporting in CI |
| P3 | Performance benchmarks | Add benchmarks for critical paths |
| P3 | API documentation | Generate Rust docs |
| P3 | Security scanning | Integrate SAST tools |

### Architecture Improvements

1. **Consider splitting large services**:
   - `import_service.rs` (1,333 LOC) could be split by import type
   - `agent_executor.rs` (1,156 LOC) could extract tool implementations

2. **State management refinement**:
   - Consider using Tauri's managed state more extensively
   - Reduce reliance on `lazy_static` globals

3. **Error handling standardization**:
   - Unify error types across all services
   - Implement consistent error codes for frontend

---

## Appendix A: File Tree Reference

```
apps/desktop/src-tauri/
├── Cargo.toml
├── build.rs
├── tauri.conf.json
├── capabilities/
│   └── default.json
├── icons/
└── src/
    ├── main.rs
    ├── lib.rs
    ├── menu.rs
    ├── commands/
    │   ├── mod.rs
    │   ├── fs.rs
    │   ├── workspace.rs
    │   ├── versions.rs
    │   ├── images.rs
    │   ├── llm.rs
    │   ├── auth.rs
    │   ├── agent.rs
    │   ├── import.rs
    │   ├── export.rs
    │   ├── recovery.rs
    │   ├── file_watcher.rs
    │   ├── error_reporter.rs
    │   ├── system.rs
    │   └── updates.rs
    └── services/
        ├── mod.rs
        ├── error.rs
        ├── workspace_manager.rs
        ├── checkpoint_manager.rs
        ├── object_store.rs
        ├── recovery_manager.rs
        ├── auth_service.rs
        ├── llm_service.rs
        ├── agent_executor.rs
        ├── import_service.rs
        ├── import_transaction.rs
        ├── import_security.rs
        ├── docx_import.rs
        ├── docx_export.rs
        ├── file_watcher.rs
        ├── image_manager.rs
        └── error_reporter.rs
```

---

## Appendix B: Workspace Data Model

```
project/
├── documents/
│   ├── notes/
│   │   ├── meeting.md
│   │   └── meeting.sidecar.json
│   └── ideas.md
└── .midlight/
    ├── workspace.config.json
    ├── objects/
    │   ├── ab/
    │   │   └── cd1234...  (gzipped content)
    │   └── ef/
    │       └── gh5678...
    ├── checkpoints/
    │   └── notes/
    │       └── meeting.md.json
    ├── images/
    │   └── abc123...  (deduplicated)
    └── recovery/
        └── notes_meeting.md.wal
```

---

## Appendix C: Event System

| Event | Direction | Payload | Purpose |
|-------|-----------|---------|---------|
| `llm:stream` | Backend→Frontend | `{ streamId, chunk }` | LLM streaming |
| `llm:stream_end` | Backend→Frontend | `{ streamId }` | Stream complete |
| `auth:oauth-success` | Backend→Frontend | `User` | OAuth complete |
| `import:progress` | Backend→Frontend | `ImportProgress` | Import status |
| `file:external_change` | Backend→Frontend | `{ path }` | File modified |
| `update:available` | Backend→Frontend | `UpdateInfo` | New version |

---

*End of Audit Report*
