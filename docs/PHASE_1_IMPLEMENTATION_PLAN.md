# Phase 1: Foundation Implementation Plan

**Document Version:** 1.0
**Created:** 2026-01-10
**Target Codebase:** midlight-next/
**Estimated Duration:** 2 weeks

---

## Overview

Phase 1 establishes the project and context infrastructure. This includes defining the `ProjectConfig` type, detecting projects in the workspace, creating the project store, updating the sidebar with visual indicators, implementing `me.midlight` auto-detection, and adding new settings fields.

---

## Existing Code Analysis

### 1. Type Definitions (`packages/core/src/types/index.ts`)

**Key Patterns:**
- Interfaces use `version: number` for schema versioning (e.g., `MidlightDocument`, `SidecarDocument`, `WorkspaceConfig`)
- Metadata interfaces use ISO date strings for timestamps (`created`, `modified`)
- Configuration interfaces are organized hierarchically (e.g., `WorkspaceConfig` -> `VersioningConfig`)
- Types use union types for status enums (e.g., `'auto' | 'bookmark'`)
- `FileNode` is the core file tree type with `id`, `name`, `path`, `type`, and optional `category` and `children`

**Integration Points:**
- `FileNode` needs extension to include project metadata
- New types should follow the existing versioned interface pattern
- Export new types at the end of the file alongside existing exports

### 2. Store Patterns (`packages/stores/src/`)

**Key Patterns:**
- Stores are created using `createXXXStore()` factory functions
- State interfaces define the complete store shape (e.g., `FileSystemState`, `SettingsState`)
- Use `writable()` from Svelte with `subscribe`, `set`, `update`
- Methods are attached to the returned object for actions
- Derived stores are created using `derived()` for computed values
- Persistence uses `localStorage` with `loadPersistedSettings()`/`persistSettings()` pattern
- Platform adapters are set via `setStorageAdapter()` pattern

**Integration Points:**
- New stores should follow the `createXXXStore()` pattern
- Export stores and derived stores from `index.ts`
- Type exports should be separate from value exports

### 3. Rust Workspace Manager (`apps/desktop/src-tauri/src/services/workspace_manager.rs`)

**Key Patterns:**
- `WorkspaceManager` struct manages a single workspace with `workspace_root` and `midlight_dir` paths
- `init()` method creates `.midlight/` directory structure and default config
- `WorkspaceManagerRegistry` manages multiple workspaces with `get_or_create()` pattern
- File operations use `std::fs` with `Result<T>` error handling
- JSON serialization uses `serde_json::json!()` macro for creating structures
- Uses `chrono::Utc::now().to_rfc3339()` for timestamps
- Directory traversal would be added in the `init()` method or a new scanning method

**Integration Points:**
- Add project detection during `init()` or create new `scan_projects()` method
- Extend file loading to check for `.project.midlight` files
- Return project information alongside file tree to frontend

### 4. Sidebar Component (`apps/desktop/src/lib/components/Sidebar.svelte`)

**Key Patterns:**
- Uses Svelte 5 runes (`$state`, `$derived`, `$effect`)
- `TreeNode` extends `FileNode` with UI state like `expanded`
- File icon selection via `getFileIcon()` returns `{ icon: string, color: string }`
- Visual states use Tailwind classes conditionally
- Context menu integration for file operations
- Drag-and-drop support with state management

**Integration Points:**
- Extend `getFileIcon()` to handle project folders
- Add visual indicator (icon, badge, or color) for project folders
- Potentially add project status indicator (active/paused/archived)

### 5. Settings Modal (`apps/desktop/src/lib/components/SettingsModal.svelte`)

**Key Patterns:**
- Tab-based navigation with `activeTab` state
- Each tab has a dedicated section with consistent layout:
  - Label/description on left, control on right
  - `border-b border-border` separator between items
  - Toggle switches use custom button with `role="switch"`
- Settings are updated via `settings.setXXX()` methods
- Form controls include: toggle switches, select dropdowns, text inputs
- Sections are wrapped in `<div class="space-y-6">`

**Integration Points:**
- Add new "Context" tab for context-related settings
- Follow existing toggle switch pattern for boolean settings
- Add "Edit Profile" button that opens `me.midlight`

---

## Task Implementation Details

### Task 1: Define ProjectConfig Type

**Location:** `/Users/scottdaly/Documents/code/midlight/midlight-next/packages/core/src/types/index.ts`

**Type Definition:**

```typescript
// Add after WorkspaceConfig (around line 182)

// Project types
export type ProjectStatus = 'active' | 'paused' | 'archived';

export interface ProjectContextSettings {
  includeGlobalContext: boolean;
  autoUpdateContext: boolean;
  askBeforeUpdating: boolean;
}

export interface ProjectConfig {
  version: 1;
  name: string;
  icon?: string;           // emoji or icon identifier
  color?: string;          // hex color for UI accent
  status: ProjectStatus;
  createdAt: string;       // ISO date
  workflowSource?: string; // workflow ID that created this project
  context: ProjectContextSettings;
}

// Extended FileNode for projects
export interface ProjectNode extends FileNode {
  isProject: true;
  projectConfig: ProjectConfig;
}

// Context layer types (for Phase 2 prep)
export type ContextLayerType = 'global' | 'project' | 'document' | 'mentioned' | 'selection';

export interface ContextLayer {
  type: ContextLayerType;
  enabled: boolean;
  content: string;
  source: string;
  priority: number;
  tokenCount?: number;
}
```

**Rationale:**
- Follows existing versioned interface pattern
- `ProjectStatus` as union type matches existing enum patterns
- `ProjectNode` extends `FileNode` for type-safe project identification
- Context types added for Phase 2 preparation

---

### Task 2: Add Project Detection to workspace_manager.rs

**Location:** `/Users/scottdaly/Documents/code/midlight/midlight-next/apps/desktop/src-tauri/src/services/workspace_manager.rs`

**Step 1: Add Project Struct Definitions**

Add after the imports (around line 14):

```rust
use serde::{Deserialize, Serialize};

/// Project configuration stored in .project.midlight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub version: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub status: String, // "active" | "paused" | "archived"
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "workflowSource")]
    pub workflow_source: Option<String>,
    pub context: ProjectContextSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContextSettings {
    #[serde(rename = "includeGlobalContext")]
    pub include_global_context: bool,
    #[serde(rename = "autoUpdateContext")]
    pub auto_update_context: bool,
    #[serde(rename = "askBeforeUpdating")]
    pub ask_before_updating: bool,
}

/// Project information returned to frontend
#[derive(Debug, Clone, Serialize)]
pub struct ProjectInfo {
    pub path: String,
    pub config: ProjectConfig,
}
```

**Step 2: Add Project Detection Method to WorkspaceManager**

Add this method to the `impl WorkspaceManager` block (around line 675):

```rust
/// Scans workspace for projects (.project.midlight files)
pub fn scan_projects(&self) -> Result<Vec<ProjectInfo>> {
    let mut projects = Vec::new();
    self.scan_projects_recursive(&self.workspace_root, &mut projects)?;
    Ok(projects)
}

fn scan_projects_recursive(&self, dir: &Path, projects: &mut Vec<ProjectInfo>) -> Result<()> {
    let project_file = dir.join(".project.midlight");

    if project_file.exists() {
        if let Ok(content) = fs::read_to_string(&project_file) {
            if let Ok(config) = serde_json::from_str::<ProjectConfig>(&content) {
                let relative_path = dir
                    .strip_prefix(&self.workspace_root)
                    .unwrap_or(dir)
                    .to_string_lossy()
                    .to_string();

                projects.push(ProjectInfo {
                    path: if relative_path.is_empty() { ".".to_string() } else { relative_path },
                    config,
                });
            }
        }
    }

    // Recursively scan subdirectories
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip hidden directories except .midlight
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with('.') && name != ".midlight" {
                    continue;
                }
                self.scan_projects_recursive(&path, projects)?;
            }
        }
    }

    Ok(())
}

/// Checks if a path is a project (contains .project.midlight)
pub fn is_project(&self, relative_path: &str) -> bool {
    let full_path = self.workspace_root.join(relative_path);
    full_path.join(".project.midlight").exists()
}

/// Gets project config for a path
pub fn get_project_config(&self, relative_path: &str) -> Result<Option<ProjectConfig>> {
    let project_file = self.workspace_root.join(relative_path).join(".project.midlight");

    if !project_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&project_file)?;
    let config: ProjectConfig = serde_json::from_str(&content)?;
    Ok(Some(config))
}
```

**Step 3: Add Tauri Command for Project Scanning**

Add to `/Users/scottdaly/Documents/code/midlight/midlight-next/apps/desktop/src-tauri/src/commands/workspace.rs`:

```rust
#[tauri::command]
pub async fn scan_projects(
    workspace_root: String,
    workspace_registry: tauri::State<'_, WorkspaceRegistry>,
) -> Result<Vec<ProjectInfo>, String> {
    let registry = workspace_registry.0.read().await;
    let manager = registry
        .get(&workspace_root)
        .ok_or_else(|| "Workspace not initialized".to_string())?;

    manager.scan_projects().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn is_project(
    workspace_root: String,
    path: String,
    workspace_registry: tauri::State<'_, WorkspaceRegistry>,
) -> Result<bool, String> {
    let registry = workspace_registry.0.read().await;
    let manager = registry
        .get(&workspace_root)
        .ok_or_else(|| "Workspace not initialized".to_string())?;

    Ok(manager.is_project(&path))
}
```

---

### Task 3: Create Project Store

**Location:** New file `/Users/scottdaly/Documents/code/midlight/midlight-next/packages/stores/src/project.ts`

```typescript
// @midlight/stores/project - Project state management

import { writable, derived, get } from 'svelte/store';
import type { ProjectConfig, ProjectStatus } from '@midlight/core/types';

export interface ProjectInfo {
  path: string;
  config: ProjectConfig;
}

export interface ProjectState {
  projects: ProjectInfo[];
  isScanning: boolean;
  error: string | null;
}

const initialState: ProjectState = {
  projects: [],
  isScanning: false,
  error: null,
};

// Project scanner function type (injected from platform layer)
export type ProjectScanner = (workspaceRoot: string) => Promise<ProjectInfo[]>;

function createProjectStore() {
  const { subscribe, set, update } = writable<ProjectState>(initialState);

  // Project scanner will be set based on platform (Tauri or Web)
  let projectScanner: ProjectScanner | null = null;

  return {
    subscribe,

    /**
     * Sets the project scanner function (Tauri or Web)
     */
    setProjectScanner(scanner: ProjectScanner) {
      projectScanner = scanner;
    },

    /**
     * Scans workspace for projects
     */
    async scanProjects(workspaceRoot: string) {
      if (!projectScanner) {
        update(s => ({ ...s, error: 'Project scanner not initialized' }));
        return;
      }

      update(s => ({ ...s, isScanning: true, error: null }));

      try {
        const projects = await projectScanner(workspaceRoot);
        update(s => ({
          ...s,
          projects,
          isScanning: false,
        }));
      } catch (error) {
        update(s => ({
          ...s,
          isScanning: false,
          error: error instanceof Error ? error.message : String(error),
        }));
      }
    },

    /**
     * Checks if a path is a known project
     */
    isProject(path: string): boolean {
      const state = get({ subscribe });
      return state.projects.some(p => p.path === path);
    },

    /**
     * Gets project config for a path
     */
    getProjectConfig(path: string): ProjectConfig | null {
      const state = get({ subscribe });
      const project = state.projects.find(p => p.path === path);
      return project?.config ?? null;
    },

    /**
     * Gets all projects with a specific status
     */
    getProjectsByStatus(status: ProjectStatus): ProjectInfo[] {
      const state = get({ subscribe });
      return state.projects.filter(p => p.config.status === status);
    },

    /**
     * Clears all project data
     */
    clear() {
      set(initialState);
    },

    /**
     * Resets the store
     */
    reset() {
      set(initialState);
    },
  };
}

export const projectStore = createProjectStore();

// Derived stores
export const projects = derived(projectStore, ($ps) => $ps.projects);

export const activeProjects = derived(projectStore, ($ps) =>
  $ps.projects.filter(p => p.config.status === 'active')
);

export const projectCount = derived(projectStore, ($ps) => $ps.projects.length);

export const isProjectScanning = derived(projectStore, ($ps) => $ps.isScanning);

export const projectPaths = derived(projectStore, ($ps) =>
  new Set($ps.projects.map(p => p.path))
);
```

**Add to index.ts exports:**

```typescript
// Add to packages/stores/src/index.ts
export {
  projectStore,
  projects,
  activeProjects,
  projectCount,
  isProjectScanning,
  projectPaths,
} from './project.js';
export type { ProjectInfo, ProjectState, ProjectScanner } from './project.js';
```

---

### Task 4: Update Sidebar with Project Indicators

**Location:** `/Users/scottdaly/Documents/code/midlight/midlight-next/apps/desktop/src/lib/components/Sidebar.svelte`

**Step 1: Import project store**

Add to the imports (around line 4):

```typescript
import { fileSystem, activeFile, selectedPaths, settings, pendingNewItem, pendingChanges, projectPaths } from '@midlight/stores';
```

**Step 2: Update getFileIcon function**

Modify the `getFileIcon` function (around line 486) to handle projects:

```typescript
function getFileIcon(node: TreeNode): { icon: string; color: string; isProject?: boolean } {
  // Check if this is a project folder
  if (node.type === 'directory' && $projectPaths.has(node.path)) {
    return { icon: 'project', color: 'text-primary', isProject: true };
  }

  if (node.type === 'directory') {
    return { icon: 'folder', color: 'text-muted-foreground' };
  }

  switch (node.category) {
    case 'midlight':
      return { icon: 'midlight', color: 'text-primary' };
    case 'native':
      return { icon: 'markdown', color: 'text-blue-500' };
    case 'importable':
      return { icon: 'document', color: 'text-orange-500' };
    case 'viewable':
      return { icon: 'image', color: 'text-green-500' };
    default:
      return { icon: 'file', color: 'text-muted-foreground' };
  }
}
```

**Step 3: Add project icon SVG in the file node snippet**

Update the folder icon section in the `fileNode` snippet (around line 746) to include project icon:

```svelte
{#if node.type === 'directory'}
  <!-- Expand/collapse chevron -->
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    class="flex-shrink-0 transition-transform text-muted-foreground {node.expanded ? 'rotate-90' : ''}"
  >
    <polyline points="9 18 15 12 9 6"/>
  </svg>
  <!-- Folder/Project icon -->
  {#if iconInfo.isProject}
    <!-- Project folder icon - briefcase style -->
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 {iconInfo.color}">
      <rect x="2" y="7" width="20" height="14" rx="2" ry="2"/>
      <path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16"/>
    </svg>
  {:else}
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 text-amber-500">
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
    </svg>
  {/if}
{:else}
```

**Step 4: Initialize project scanning when workspace loads**

In the App.svelte or wherever workspace initialization happens, add project scanning after loading the directory:

```typescript
// After fileSystem.loadDir(path)
await projectStore.scanProjects(path);
```

---

### Task 5: Add me.midlight Auto-Detection and Template Creation

**Location:** `/Users/scottdaly/Documents/code/midlight/midlight-next/apps/desktop/src-tauri/src/services/workspace_manager.rs`

**Step 1: Add me.midlight detection and creation to init()**

Update the `init()` method to check for and optionally create `me.midlight`:

```rust
/// Initialize the workspace (.midlight folder structure)
pub async fn init(&self) -> Result<()> {
    // Create .midlight directory structure
    fs::create_dir_all(&self.midlight_dir)?;
    fs::create_dir_all(self.midlight_dir.join("objects"))?;
    fs::create_dir_all(self.midlight_dir.join("checkpoints"))?;
    fs::create_dir_all(self.midlight_dir.join("images"))?;
    fs::create_dir_all(self.midlight_dir.join("recovery"))?;

    // Initialize services
    self.object_store.init().await?;
    self.checkpoint_manager.write().await.init().await?;

    // Create default config if not exists
    let config_path = self.midlight_dir.join("workspace.config.json");
    if !config_path.exists() {
        let default_config = serde_json::json!({
            "version": 1,
            "versioning": {
                "enabled": true,
                "autoCheckpointInterval": 300,
                "minChangeThreshold": 50,
                "maxCheckpointsPerFile": 50,
                "retentionDays": 7
            },
            "editor": {
                "defaultFont": "Inter",
                "defaultFontSize": "16px",
                "spellcheck": true,
                "autoSave": true,
                "autoSaveInterval": 3000
            },
            "recovery": {
                "enabled": true,
                "walInterval": 500
            }
        });
        fs::write(config_path, serde_json::to_string_pretty(&default_config)?)?;
    }

    // Check for me.midlight and create template if not exists
    self.ensure_me_midlight()?;

    tracing::info!("Initialized workspace: {}", self.workspace_root.display());

    Ok(())
}

/// Ensures me.midlight exists with template content
fn ensure_me_midlight(&self) -> Result<()> {
    let me_path = self.workspace_root.join("me.midlight");

    if me_path.exists() {
        return Ok(());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let template = serde_json::json!({
        "version": 1,
        "meta": {
            "created": now,
            "modified": now,
            "title": "About Me"
        },
        "document": {
            "defaultFont": "Merriweather",
            "defaultFontSize": 16
        },
        "content": {
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": { "level": 1 },
                    "content": [{ "type": "text", "text": "About Me" }]
                },
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "Tell the AI about yourself so it can provide more personalized assistance." }]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "Basics" }]
                },
                {
                    "type": "bulletList",
                    "content": [
                        {
                            "type": "listItem",
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": "Name: " }]
                            }]
                        },
                        {
                            "type": "listItem",
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": "Location: " }]
                            }]
                        },
                        {
                            "type": "listItem",
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": "Occupation: " }]
                            }]
                        }
                    ]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "Interests" }]
                },
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "What topics are you most interested in?" }]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "Communication Preferences" }]
                },
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "How would you like the AI to communicate with you? (e.g., formal/casual, detailed/concise)" }]
                }
            ]
        },
        "images": {}
    });

    fs::write(&me_path, serde_json::to_string_pretty(&template)?)?;
    tracing::info!("Created me.midlight template at {}", me_path.display());

    Ok(())
}

/// Checks if me.midlight exists
pub fn has_me_midlight(&self) -> bool {
    self.workspace_root.join("me.midlight").exists()
}

/// Loads me.midlight content as Markdown for AI context
pub fn load_me_midlight_as_context(&self) -> Result<Option<String>> {
    let me_path = self.workspace_root.join("me.midlight");

    if !me_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&me_path)?;
    let doc: serde_json::Value = serde_json::from_str(&content)?;

    // Extract content and convert to markdown for context
    if let Some(content) = doc.get("content") {
        let markdown = self.tiptap_to_markdown(content);
        Ok(Some(markdown))
    } else {
        Ok(None)
    }
}
```

---

### Task 6: Add context.midlight Template Structure

**Location:** `/Users/scottdaly/Documents/code/midlight/midlight-next/apps/desktop/src-tauri/src/services/workspace_manager.rs`

Add a method to create context.midlight when creating a project:

```rust
/// Creates context.midlight with structured template for a project
pub fn create_context_template(&self, project_path: &str) -> Result<()> {
    let context_path = self.workspace_root.join(project_path).join("context.midlight");

    if context_path.exists() {
        return Ok(());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let template = serde_json::json!({
        "version": 1,
        "meta": {
            "created": now,
            "modified": now,
            "title": "Project Context"
        },
        "document": {
            "defaultFont": "Merriweather",
            "defaultFontSize": 16
        },
        "content": {
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": { "level": 1 },
                    "content": [{ "type": "text", "text": "Project Context" }]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "Overview" }]
                },
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "Describe the high-level goal and scope of this project." }]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "Current Status" }]
                },
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "Where things stand right now." }]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "Key Decisions" }]
                },
                {
                    "type": "bulletList",
                    "content": [
                        {
                            "type": "listItem",
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": "[Date]: [Decision description]" }]
                            }]
                        }
                    ]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "Open Questions" }]
                },
                {
                    "type": "taskList",
                    "content": [
                        {
                            "type": "taskItem",
                            "attrs": { "checked": false },
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": "Question 1" }]
                            }]
                        }
                    ]
                },
                {
                    "type": "heading",
                    "attrs": { "level": 2 },
                    "content": [{ "type": "text", "text": "AI Notes" }]
                },
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "Meta-instructions for how the AI should behave in this project. For example: \"Be concise\" or \"Ask clarifying questions before making suggestions.\"" }]
                }
            ]
        },
        "images": {}
    });

    // Ensure parent directory exists
    if let Some(parent) = context_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&context_path, serde_json::to_string_pretty(&template)?)?;
    tracing::info!("Created context.midlight template at {}", context_path.display());

    Ok(())
}

/// Creates a new project with .project.midlight and context.midlight
pub fn create_project(&self, project_path: &str, name: &str, workflow_source: Option<&str>) -> Result<ProjectConfig> {
    let full_path = self.workspace_root.join(project_path);

    // Create directory if it doesn't exist
    fs::create_dir_all(&full_path)?;

    let now = chrono::Utc::now().to_rfc3339();

    let config = ProjectConfig {
        version: 1,
        name: name.to_string(),
        icon: None,
        color: None,
        status: "active".to_string(),
        created_at: now,
        workflow_source: workflow_source.map(|s| s.to_string()),
        context: ProjectContextSettings {
            include_global_context: true,
            auto_update_context: true,
            ask_before_updating: false,
        },
    };

    let project_file = full_path.join(".project.midlight");
    fs::write(&project_file, serde_json::to_string_pretty(&config)?)?;

    // Create context.midlight
    self.create_context_template(project_path)?;

    tracing::info!("Created project at {}", full_path.display());

    Ok(config)
}
```

---

### Task 7: Add New Settings Fields

**Location:** `/Users/scottdaly/Documents/code/midlight/midlight-next/packages/stores/src/settings.ts`

Update the `SettingsState` interface and defaults:

```typescript
export interface SettingsState {
  // Existing fields
  isOpen: boolean;
  theme: Theme;
  pageMode: PageMode;
  fontSize: number;
  fontFamily: string;
  spellcheck: boolean;
  autoSave: boolean;
  autoSaveInterval: number;
  showLineNumbers: boolean;
  errorReportingEnabled: boolean;
  apiKey: string;

  // New: Storage Settings
  rootFolderLocation: string;

  // New: Context Settings
  autoUpdateProjectContext: boolean;
  askBeforeSavingContext: boolean;
  showContextUpdateNotifications: boolean;
  learnAboutMeAutomatically: boolean;
  includeGlobalContext: boolean;
}

const defaultSettings: SettingsState = {
  // Existing defaults
  isOpen: false,
  theme: 'system',
  pageMode: 'normal',
  fontSize: 16,
  fontFamily: 'Merriweather',
  spellcheck: true,
  autoSave: true,
  autoSaveInterval: 3000,
  showLineNumbers: false,
  errorReportingEnabled: false,
  apiKey: '',

  // New defaults
  rootFolderLocation: '',  // Empty means use default (Documents/Midlight/)
  autoUpdateProjectContext: true,
  askBeforeSavingContext: false,
  showContextUpdateNotifications: false,
  learnAboutMeAutomatically: true,
  includeGlobalContext: true,
};
```

Add new setter methods to the store:

```typescript
/**
 * Sets root folder location
 */
setRootFolderLocation(location: string) {
  update((s) => ({ ...s, rootFolderLocation: location }));
},

/**
 * Sets auto-update project context
 */
setAutoUpdateProjectContext(enabled: boolean) {
  update((s) => ({ ...s, autoUpdateProjectContext: enabled }));
},

/**
 * Sets ask before saving context
 */
setAskBeforeSavingContext(enabled: boolean) {
  update((s) => ({ ...s, askBeforeSavingContext: enabled }));
},

/**
 * Sets show context update notifications
 */
setShowContextUpdateNotifications(enabled: boolean) {
  update((s) => ({ ...s, showContextUpdateNotifications: enabled }));
},

/**
 * Sets learn about me automatically
 */
setLearnAboutMeAutomatically(enabled: boolean) {
  update((s) => ({ ...s, learnAboutMeAutomatically: enabled }));
},

/**
 * Sets include global context
 */
setIncludeGlobalContext(enabled: boolean) {
  update((s) => ({ ...s, includeGlobalContext: enabled }));
},
```

---

### Task 8: Update SettingsModal

**Location:** `/Users/scottdaly/Documents/code/midlight/midlight-next/apps/desktop/src/lib/components/SettingsModal.svelte`

**Step 1: Add 'context' tab**

Update the tabs array (around line 92):

```typescript
type Tab = 'appearance' | 'editor' | 'ai' | 'context' | 'general' | 'shortcuts';

const tabs: { id: Tab; label: string }[] = [
  { id: 'appearance', label: 'Appearance' },
  { id: 'editor', label: 'Editor' },
  { id: 'ai', label: 'AI' },
  { id: 'context', label: 'Context' },
  { id: 'general', label: 'General' },
  { id: 'shortcuts', label: 'Shortcuts' },
];
```

**Step 2: Add icon for context tab in the nav**

Add after the AI tab icon (around line 195):

```svelte
{:else if tab.id === 'context'}
  <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <circle cx="12" cy="12" r="10"/>
    <path d="M12 16v-4"/>
    <path d="M12 8h.01"/>
  </svg>
```

**Step 3: Add Context tab content**

Add after the AI tab content section (around line 575):

```svelte
{:else if activeTab === 'context'}
  <!-- Context Settings -->
  <div class="space-y-6">
    <!-- Global Context Section -->
    <div class="py-3 border-b border-border">
      <h4 class="text-sm font-medium mb-4">Global Context (me.midlight)</h4>

      <!-- Include Global Context -->
      <div class="flex items-center justify-between py-3">
        <div>
          <div class="text-sm font-medium">Include in AI conversations</div>
          <div class="text-xs text-muted-foreground">Share your profile info with the AI for personalized responses</div>
        </div>
        <button
          onclick={() => settings.setIncludeGlobalContext(!$settings.includeGlobalContext)}
          role="switch"
          aria-checked={$settings.includeGlobalContext}
          aria-label="Toggle include global context"
          class="relative w-11 h-6 rounded-full transition-colors {$settings.includeGlobalContext ? 'bg-primary' : 'bg-muted'}"
        >
          <span
            class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.includeGlobalContext ? 'translate-x-5' : 'translate-x-0'}"
          ></span>
        </button>
      </div>

      <!-- Learn About Me Automatically -->
      <div class="flex items-center justify-between py-3">
        <div>
          <div class="text-sm font-medium">Learn about me automatically</div>
          <div class="text-xs text-muted-foreground">AI can update your profile based on conversations</div>
        </div>
        <button
          onclick={() => settings.setLearnAboutMeAutomatically(!$settings.learnAboutMeAutomatically)}
          role="switch"
          aria-checked={$settings.learnAboutMeAutomatically}
          aria-label="Toggle learn about me"
          class="relative w-11 h-6 rounded-full transition-colors {$settings.learnAboutMeAutomatically ? 'bg-primary' : 'bg-muted'}"
        >
          <span
            class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.learnAboutMeAutomatically ? 'translate-x-5' : 'translate-x-0'}"
          ></span>
        </button>
      </div>

      <!-- Edit Profile Button -->
      <div class="pt-3">
        <button
          onclick={() => {
            // TODO: Open me.midlight in editor
            onClose();
          }}
          class="px-4 py-2 text-sm border border-border rounded-md hover:bg-accent transition-colors"
        >
          Edit Profile
        </button>
      </div>
    </div>

    <!-- Project Context Section -->
    <div class="py-3 border-b border-border">
      <h4 class="text-sm font-medium mb-4">Project Context</h4>

      <!-- Auto-update Project Context -->
      <div class="flex items-center justify-between py-3">
        <div>
          <div class="text-sm font-medium">Auto-update project context</div>
          <div class="text-xs text-muted-foreground">AI automatically maintains context.midlight with key decisions and status</div>
        </div>
        <button
          onclick={() => settings.setAutoUpdateProjectContext(!$settings.autoUpdateProjectContext)}
          role="switch"
          aria-checked={$settings.autoUpdateProjectContext}
          aria-label="Toggle auto-update project context"
          class="relative w-11 h-6 rounded-full transition-colors {$settings.autoUpdateProjectContext ? 'bg-primary' : 'bg-muted'}"
        >
          <span
            class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.autoUpdateProjectContext ? 'translate-x-5' : 'translate-x-0'}"
          ></span>
        </button>
      </div>

      <!-- Ask Before Saving Context -->
      {#if $settings.autoUpdateProjectContext}
        <div class="flex items-center justify-between py-3">
          <div>
            <div class="text-sm font-medium">Ask before saving changes</div>
            <div class="text-xs text-muted-foreground">Prompt for confirmation before AI updates context</div>
          </div>
          <button
            onclick={() => settings.setAskBeforeSavingContext(!$settings.askBeforeSavingContext)}
            role="switch"
            aria-checked={$settings.askBeforeSavingContext}
            aria-label="Toggle ask before saving context"
            class="relative w-11 h-6 rounded-full transition-colors {$settings.askBeforeSavingContext ? 'bg-primary' : 'bg-muted'}"
          >
            <span
              class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.askBeforeSavingContext ? 'translate-x-5' : 'translate-x-0'}"
            ></span>
          </button>
        </div>
      {/if}

      <!-- Show Context Update Notifications -->
      {#if $settings.autoUpdateProjectContext}
        <div class="flex items-center justify-between py-3">
          <div>
            <div class="text-sm font-medium">Show update notifications</div>
            <div class="text-xs text-muted-foreground">Display a toast when context is updated</div>
          </div>
          <button
            onclick={() => settings.setShowContextUpdateNotifications(!$settings.showContextUpdateNotifications)}
            role="switch"
            aria-checked={$settings.showContextUpdateNotifications}
            aria-label="Toggle show context update notifications"
            class="relative w-11 h-6 rounded-full transition-colors {$settings.showContextUpdateNotifications ? 'bg-primary' : 'bg-muted'}"
          >
            <span
              class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.showContextUpdateNotifications ? 'translate-x-5' : 'translate-x-0'}"
            ></span>
          </button>
        </div>
      {/if}
    </div>

    <!-- Info Section -->
    <div class="py-3">
      <p class="text-xs text-muted-foreground">
        Context helps the AI understand your preferences and project history. All context files are regular documents that you can view and edit at any time.
      </p>
    </div>
  </div>
```

---

## Summary Checklist

### Task 1: Define ProjectConfig Type
- [ ] Add `ProjectStatus` type
- [ ] Add `ProjectContextSettings` interface
- [ ] Add `ProjectConfig` interface
- [ ] Add `ProjectNode` interface extending FileNode
- [ ] Add `ContextLayerType` type
- [ ] Add `ContextLayer` interface

### Task 2: Add Project Detection to workspace_manager.rs
- [ ] Add Rust struct definitions (`ProjectConfig`, `ProjectContextSettings`, `ProjectInfo`)
- [ ] Add `scan_projects()` method
- [ ] Add `scan_projects_recursive()` helper method
- [ ] Add `is_project()` method
- [ ] Add `get_project_config()` method
- [ ] Add Tauri commands for project operations

### Task 3: Create Project Store
- [ ] Create `/packages/stores/src/project.ts`
- [ ] Define `ProjectState` interface
- [ ] Implement `createProjectStore()` factory
- [ ] Add derived stores (`projects`, `activeProjects`, `projectCount`, `projectPaths`)
- [ ] Export from `index.ts`

### Task 4: Update Sidebar with Project Indicators
- [ ] Import `projectPaths` from stores
- [ ] Modify `getFileIcon()` to detect projects
- [ ] Add project icon SVG
- [ ] Initialize project scanning on workspace load

### Task 5: Add me.midlight Auto-Detection and Template Creation
- [ ] Add `ensure_me_midlight()` method
- [ ] Add `has_me_midlight()` method
- [ ] Add `load_me_midlight_as_context()` method
- [ ] Call `ensure_me_midlight()` in `init()`

### Task 6: Add context.midlight Template Structure
- [ ] Add `create_context_template()` method
- [ ] Add `create_project()` method
- [ ] Include structured sections (Overview, Status, Decisions, Questions, AI Notes)

### Task 7: Add New Settings Fields
- [ ] Add `rootFolderLocation` field
- [ ] Add `autoUpdateProjectContext` field
- [ ] Add `askBeforeSavingContext` field
- [ ] Add `showContextUpdateNotifications` field
- [ ] Add `learnAboutMeAutomatically` field
- [ ] Add `includeGlobalContext` field
- [ ] Add setter methods for each new field

### Task 8: Update SettingsModal
- [ ] Add 'context' to Tab type
- [ ] Add Context tab to tabs array
- [ ] Add Context tab icon
- [ ] Add Context tab content with all toggles
- [ ] Add "Edit Profile" button functionality

---

## Dependencies Between Tasks

```
Task 1 (Types)
    |
    +---> Task 2 (Rust detection)
    |         |
    |         +---> Task 3 (Project store)
    |                   |
    |                   +---> Task 4 (Sidebar UI)
    |
    +---> Task 5 (me.midlight)
    |
    +---> Task 6 (context.midlight)

Task 7 (Settings fields) ---> Task 8 (Settings UI)
```

Recommended implementation order:
1. Task 1 (types - enables all other tasks)
2. Task 7 (settings - independent)
3. Task 5 (me.midlight - independent of project system)
4. Task 2 (Rust - depends on Task 1)
5. Task 3 (store - depends on Task 2)
6. Task 4 (UI - depends on Task 3)
7. Task 6 (context template - can be done after Task 5)
8. Task 8 (settings UI - depends on Task 7)

---

## Testing Recommendations

### Unit Tests
- Test `ProjectConfig` type validation
- Test project store methods (`isProject`, `getProjectConfig`, `getProjectsByStatus`)
- Test settings persistence with new fields

### Integration Tests
- Test project detection during workspace scan
- Test me.midlight creation on workspace init
- Test context.midlight creation on project creation

### Manual Testing Checklist
- [ ] Create new workspace, verify me.midlight is created
- [ ] Create folder with .project.midlight, verify project icon appears
- [ ] Open Settings, verify Context tab appears with all toggles
- [ ] Toggle each context setting, verify persistence after restart
- [ ] Create project via API, verify context.midlight is created

---

*Document prepared for Phase 1 implementation. Proceed task-by-task, testing each component before moving to the next.*
