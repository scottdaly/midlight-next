# Midlight Vision Alignment Implementation Plan

**Document Version:** 2.0
**Created:** 2026-01-10
**Last Updated:** 2026-01-10
**Target Codebase:** midlight-next/

---

## Executive Summary

This document outlines a comprehensive implementation plan to align the midlight-next codebase with the Midlight Vision document. The plan addresses six major feature gaps and provides architectural recommendations, implementation options, and a phased development roadmap.

### Current State Assessment

| Feature | Status | Notes |
|---------|--------|-------|
| Document format (.midlight) | ✅ Implemented | JSON storage with Tiptap, Markdown translation |
| AI/Agent system | ✅ Implemented | 7 tools, streaming, staged edits |
| Version control | ✅ Implemented | Checkpoints, bookmarks, object store |
| Import/Export | ✅ Implemented | Obsidian, Notion, DOCX |
| Recovery system | ✅ Implemented | WAL-based auto-recovery |
| Projects (.project.midlight) | ✅ Implemented | Phase 1 - Project-as-first-class-object |
| me.midlight management | ✅ Implemented | Phase 1 - Global user context |
| Settings (context-related) | ✅ Implemented | Phase 1 - New context settings |
| Context hierarchy assembly | ✅ Implemented | Phase 2 - Layered context at AI boundary |
| Fresh Start mode | ✅ Implemented | Phase 2 - Temporary context disable |
| Context parsing | ✅ Implemented | Phase 3 - Structured section parsing |
| Automatic context updates | ✅ Implemented | Phase 3 - Silent AI context maintenance |
| Workflows | ✅ Implemented | Phase 4 - Interview-based scaffolding |
| RAG Integration | ✅ Implemented | Phase 5 - Embedding-based semantic search |
| Project Status UI | ✅ Implemented | Phase 5 - Status management in sidebar |
| Project Archiving | ✅ Implemented | Phase 5 - Archive section with restore |
| Context Panel | ✅ Implemented | Phase 5 - Token budget, layer controls |
| Unified Search | ✅ Implemented | Phase 5 - Semantic + file search |

---

## Implementation Status

### ✅ Phase 1: Foundation (COMPLETED)

**Implemented Features:**

1. **ProjectConfig Types** (`packages/core/src/types/index.ts`)
   - `ProjectConfig` interface with version, name, icon, color, status, workflow source
   - `ProjectContextSettings` for context configuration
   - `ProjectStatus` type ('active' | 'paused' | 'archived')

2. **Project Detection** (`apps/desktop/src-tauri/src/commands/workspace.rs`)
   - `workspace_scan_projects` - Scans directory for `.project.midlight` files
   - `workspace_is_project` - Checks if a folder is a project
   - Integrated with Tauri command system in `lib.rs`

3. **Project Store** (`packages/stores/src/project.ts`)
   - `projectStore` with scan, add, remove, update, archive operations
   - Derived stores: `projects`, `activeProjects`, `pausedProjects`, `archivedProjects`
   - `ProjectScanner` interface for platform-specific scanning

4. **me.midlight Support**
   - Auto-detection in context assembly
   - Template structure for new workspaces
   - Included in global context layer

5. **Settings Updates** (`packages/stores/src/settings.ts`)
   - `autoUpdateProjectContext: boolean` (default: true)
   - `askBeforeSavingContext: boolean` (default: false)
   - `showContextUpdateNotifications: boolean` (default: false)
   - `includeGlobalContext: boolean` (default: true)

---

### ✅ Phase 2: Context Assembly (COMPLETED)

**Implemented Features:**

1. **Layered Context System** (`packages/stores/src/ai.ts`)
   - `ContextLayer` interface with type, enabled, content, source, priority
   - `ContextLayerType`: 'global' | 'project' | 'document' | 'mentioned' | 'selection'
   - Hierarchical assembly: me.midlight → context.midlight → current doc → @-mentions

2. **Context Store** (`packages/stores/src/context.ts`)
   - `contextUpdateStore` for managing context updates
   - `pendingContextUpdate` for staged updates
   - `showContextUpdateDialog` for confirmation UI
   - Undo capability with `canUndoContextUpdate`

3. **Fresh Start Mode** (`packages/stores/src/ai.ts`)
   - `freshStartMode` state flag
   - Toggle function in AI store
   - Skips global and project context when enabled
   - Natural language trigger detection patterns

4. **Exported Types** (`packages/stores/src/index.ts`)
   - `ContextLayer`, `ContextLayerType`, `GlobalContextLoader`
   - `ProjectContextLoader`, `ContextUpdateHook`

---

### ✅ Phase 3: Automatic Context Updates (COMPLETED)

**Implemented Features:**

1. **Context Parser** (`packages/core/src/context/parser.ts`)
   - `parseContextDocument()` - Parses markdown into structured sections
   - `ContextDocument` interface with overview, currentStatus, keyDecisions, openQuestions, aiNotes
   - `serializeContextDocument()` - Converts back to markdown
   - Handles date-prefixed decisions and checkbox questions

2. **Context Updater** (`packages/core/src/context/updater.ts`)
   - `ContextUpdate` type with action (add/update/remove/resolve)
   - `parseExtractionResponse()` - Parses LLM extraction responses
   - `applyUpdates()` - Applies updates to context document
   - `generateExtractionPrompt()` - Creates prompt for context extraction
   - Supports: adding decisions, updating status, resolving questions

3. **Unit Tests**
   - `packages/core/src/context/parser.test.ts` (7 tests)
   - `packages/core/src/context/updater.test.ts` (11 tests)

---

### ✅ Phase 4: Workflows (COMPLETED)

**Implemented Features:**

1. **Workflow Types** (`packages/core/src/workflows/types.ts`)
   ```typescript
   interface WorkflowDefinition {
     id: string;
     name: string;
     description: string;
     icon: string;
     category: string;
     interview: InterviewStep[];
     templates: TemplateDefinition[];
     contextSections: ContextSectionTemplates;
     projectNameTemplate?: string;
     projectColor?: string;
   }

   interface InterviewStep {
     id: string;
     question: string;
     type: 'text' | 'number' | 'select' | 'multiselect' | 'date' | 'textarea';
     options?: string[];
     required: boolean;
     placeholder?: string;
     helpText?: string;
     defaultValue?: string | number | string[];
     validation?: string;
     validationMessage?: string;
     showIf?: { stepId: string; equals?: string | string[]; notEquals?: string | string[] };
   }
   ```

2. **Workflow Executor** (`packages/core/src/workflows/executor.ts`)
   - `interpolateTemplate()` - Replaces `{{placeholders}}` with answers
   - `generateProjectName()` - Creates project name from template
   - `generateProjectConfig()` - Creates `.project.midlight` content
   - `generateContextDocument()` - Creates `context.midlight` content
   - `generateFileContent()` - Creates file content (static or LLM-generated)
   - `executeWorkflow()` - Full workflow execution with progress callbacks
   - `validateAnswers()` - Validates interview answers including conditional steps
   - `wrapInMidlightFormat()` - Converts markdown to .midlight JSON format

3. **Built-in Workflows** (`packages/core/src/workflows/definitions.ts`)

   | Workflow | Category | Interview Steps | Templates |
   |----------|----------|-----------------|-----------|
   | Weight Loss Journey | health | 6 steps | meal-plan, workout-plan, progress-log, recipes/ |
   | Big Purchase Decision | finance | 6 steps | research-notes, comparison, decision-log |
   | Book Writing Project | creative | 6 steps | outline, characters, world-building, research, chapters/, drafts/ |

   - `getWorkflowById()` - Retrieve workflow by ID
   - `getWorkflowsByCategory()` - Filter workflows by category
   - `WORKFLOW_CATEGORIES` - Built-in category definitions

4. **Workflow Store** (`packages/stores/src/workflow.ts`)
   ```typescript
   type WorkflowPhase = 'idle' | 'selecting' | 'interview' | 'executing' | 'complete' | 'error';

   interface WorkflowState {
     phase: WorkflowPhase;
     availableWorkflows: WorkflowDefinition[];
     activeWorkflow: WorkflowDefinition | null;
     currentStepIndex: number;
     answers: WorkflowAnswers;
     validationErrors: Record<string, string>;
     executionProgress: WorkflowExecutionProgress | null;
     executionResult: WorkflowExecutionResult | null;
     error: string | null;
     parentPath: string | null;
   }
   ```

   - `workflowStore` with selectWorkflow, setAnswer, nextStep, previousStep, cancel, close
   - `getCurrentVisibleStepNumber()` / `getVisibleStepCount()` for conditional step handling
   - `setFileSystem()` / `setLLMCall()` for platform integration
   - Derived stores: `workflowPhase`, `activeWorkflow`, `currentStep`, `isFirstStep`, `isLastStep`

5. **UI Components**

   - **WorkflowPicker.svelte** (`apps/desktop/src/lib/components/WorkflowPicker.svelte`)
     - Modal for workflow selection
     - Groups workflows by category
     - Shows workflow name, description, icon
     - Displays question count and file count badges
     - "Empty Project" option for blank projects

   - **WorkflowWizard.svelte** (`apps/desktop/src/lib/components/WorkflowWizard.svelte`)
     - Multi-step interview wizard
     - Supports all input types (text, textarea, number, select, multiselect)
     - Validation display
     - Progress indicator (Step X of Y)
     - Execution phase with spinner and progress bar
     - Completion phase with created files list
     - Error phase with retry option

6. **Integration**

   - **App.svelte** - Workflow initialization with file system and LLM bindings
   - **Sidebar.svelte** - "New Project..." option in folder dropdown menu

7. **Unit Tests**
   - `packages/core/src/workflows/executor.test.ts` (51 tests)
     - interpolateTemplate, generateProjectName, generateProjectConfig
     - generateContextDocument, generateFileContent, validateAnswers
     - executeWorkflow with mocked file system
   - `packages/core/src/workflows/definitions.test.ts` (86 tests)
     - getWorkflowById, getWorkflowsByCategory
     - Workflow structure validation
     - Template and interview step validation

---

### ✅ Phase 5: Polish and Cross-Project Features (COMPLETED)

**Implemented Features:**

1. **RAG Integration** (Task 1)
   - Auto-indexing on workspace load via `autoIndexProjects()` in App.svelte
   - "Index for Search" and "Re-index (force)" options in FileContextMenu
   - IndexStatusBadge component showing indexing status on project folders
   - States: not-indexed, indexing (spinner), indexed (checkmark), error

2. **Unified Search with Semantic Results** (Task 2)
   - ContextPicker enhanced with semantic search (300ms debounce)
   - Results show project badges with colors from .project.midlight
   - Content snippets with similarity scores for semantic matches
   - Combines file name fuzzy matching + RAG semantic search

3. **Context Panel Enhancement** (Task 3)
   - Token budget visualization with progress bar
   - Color coding: green < 50%, yellow < 80%, red > 80%
   - Toggle switches per layer type to enable/disable
   - "Clear all @-mentions" button
   - Added 'semantic' layer type for RAG-retrieved context

4. **Project Status UI** (Task 4)
   - Status-based icon colors in sidebar (active: blue, paused: yellow, archived: gray)
   - Status indicators on project rows (⏸ for paused, 📦 for archived)
   - Context menu with Active/Paused/Archive status options
   - Confirmation dialog for archiving

5. **Project Archiving UI** (Task 5)
   - ArchivedProjectsSection component at sidebar bottom
   - Shows count of archived projects ("X archived projects")
   - Collapsible section with restore/delete actions
   - Delete confirmation dialog for permanent removal

**Files Created (Phase 5):**

| File | Purpose |
|------|---------|
| `apps/desktop/src/lib/components/IndexStatusBadge.svelte` | RAG index status indicator |
| `apps/desktop/src/lib/components/ArchivedProjectsSection.svelte` | Archived projects list |

**Files Modified (Phase 5):**

| File | Changes |
|------|---------|
| `packages/stores/src/ai.ts` | Added 'semantic' to ContextLayerType |
| `apps/desktop/src/App.svelte` | Added autoIndexProjects() function |
| `apps/desktop/src/lib/components/Sidebar.svelte` | Added IndexStatusBadge, ArchivedProjectsSection |
| `apps/desktop/src/lib/components/FileContextMenu.svelte` | Added RAG indexing options |
| `apps/desktop/src/lib/components/Chat/ContextPicker.svelte` | Added semantic search |
| `apps/desktop/src/lib/components/ContextPanel.svelte` | Added token budget, layer toggles |

**Remaining (Backend Rust):**

| Task | Status | Notes |
|------|--------|-------|
| Project scan caching | 🔲 Pending | Add 10s TTL cache to workspace_manager.rs |
| Incremental RAG indexing | 🔲 Pending | Track file modification times in rag_service.rs |

---

## Part 1: Feature Gap Analysis and Implementation Options

### 1.1 Projects as First-Class Objects (.project.midlight)

**Status:** ✅ IMPLEMENTED

**Vision Requirement:**
> A folder becomes a project when it contains a `.project.midlight` file. This hidden configuration file stores project metadata: name, icon, color, status (active/paused/archived), creation date, workflow source, and AI context settings.

**Implementation:**
- File-based detection with in-memory caching
- Tauri commands for project scanning
- Project store for state management

**Files Implemented:**

| File | Purpose |
|------|---------|
| `packages/core/src/types/index.ts` | ProjectConfig, ProjectNode types |
| `packages/stores/src/project.ts` | Project state store |
| `apps/desktop/src-tauri/src/commands/workspace.rs` | workspace_scan_projects, workspace_is_project |

---

### 1.2 Global Context: me.midlight

**Status:** ✅ IMPLEMENTED

**Vision Requirement:**
> A single document at the root level containing persistent information about the user: name, location, job, interests, communication preferences.

**Implementation:**
- Special file treatment at workspace root
- Auto-included in AI context when present
- Template created on workspace init

---

### 1.3 Project Context: context.midlight

**Status:** ✅ IMPLEMENTED

**Vision Requirement:**
> Each project contains its own context document with structured sections: Overview, Current Status, Key Decisions, Open Questions, AI Notes.

**Implementation:**
- Structured template with predefined sections
- Context parser for reading/writing sections
- Automatic updates via context updater

**Template Structure:**
```markdown
# Project Context

## Overview
[High-level goal and scope of the project]

## Current Status
[Where things stand right now]

## Key Decisions
- [YYYY-MM-DD]: [Decision description]

## Open Questions
- [ ] [Question 1]
- [x] [Resolved question]

## AI Notes
[Meta-instructions for how the AI should behave in this project]
```

---

### 1.4 Automatic Context Updates

**Status:** ✅ IMPLEMENTED

**Vision Requirement:**
> The AI silently updates context documents as users work - appending decisions, updating status, removing resolved questions. This happens automatically by default.

**Implementation:**
- Post-response hook for context extraction
- LLM-based extraction with structured prompts
- Apply updates with undo capability
- Configurable: silent, notify, or ask-before-saving

**Files Implemented:**

| File | Purpose |
|------|---------|
| `packages/core/src/context/parser.ts` | Parse structured sections |
| `packages/core/src/context/parser.test.ts` | Parser unit tests |
| `packages/core/src/context/updater.ts` | Context update logic |
| `packages/core/src/context/updater.test.ts` | Updater unit tests |
| `packages/stores/src/context.ts` | Context update state |

---

### 1.5 Context Hierarchy Assembly

**Status:** ✅ IMPLEMENTED

**Vision Requirement:**
> When the AI responds, context is assembled in layers:
> 1. me.midlight (global)
> 2. project/context.midlight (project)
> 3. Current document(s)
> 4. @-mentioned documents

**Implementation:**
- Layer-based context assembly in AI store
- Each layer can be toggled/disabled
- Fresh Start mode skips layers 1-2

---

### 1.6 Fresh Start Mode

**Status:** ✅ IMPLEMENTED

**Vision Requirement:**
> Users can request "fresh perspectives" to temporarily disable project context, solving the problem of AI systems that anchor too heavily on past discussions.

**Implementation:**
- UI toggle in chat panel
- Skips global and project context layers
- Natural language trigger detection

---

### 1.7 Workflows System

**Status:** ✅ IMPLEMENTED

**Vision Requirement:**
> Workflows solve the cold-start problem by giving users an intuitive way to begin structured projects. Users select a workflow that interviews them and generates scaffolding.

**Implementation:**
- Hybrid approach: JSON structure + LLM personalization
- Interview-based wizard UI
- 3 built-in workflows
- Template interpolation with placeholders
- LLM-generated content for templates
- Progress tracking during execution

**Files Implemented:**

| File | Purpose |
|------|---------|
| `packages/core/src/workflows/types.ts` | Workflow type definitions |
| `packages/core/src/workflows/executor.ts` | Workflow execution engine |
| `packages/core/src/workflows/executor.test.ts` | Executor unit tests (51 tests) |
| `packages/core/src/workflows/definitions.ts` | Built-in workflows |
| `packages/core/src/workflows/definitions.test.ts` | Definitions unit tests (86 tests) |
| `packages/core/src/workflows/index.ts` | Workflow exports |
| `packages/stores/src/workflow.ts` | Workflow state store |
| `apps/desktop/src/lib/components/WorkflowPicker.svelte` | Selection UI |
| `apps/desktop/src/lib/components/WorkflowWizard.svelte` | Interview UI |

---

## Part 2: Settings Schema Update

**Status:** ✅ IMPLEMENTED

```typescript
// packages/stores/src/settings.ts

interface SettingsState {
  // Existing
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
  rootFolderLocation: string;  // default: Documents/Midlight/

  // New: Context Settings (IMPLEMENTED)
  autoUpdateProjectContext: boolean;  // default: true
  askBeforeSavingContext: boolean;    // default: false
  showContextUpdateNotifications: boolean;  // default: false
  includeGlobalContext: boolean;      // default: true
}
```

---

## Part 3: Test Coverage Summary

| Module | Test File | Tests |
|--------|-----------|-------|
| Context Parser | `packages/core/src/context/parser.test.ts` | 7 |
| Context Updater | `packages/core/src/context/updater.test.ts` | 11 |
| Workflow Executor | `packages/core/src/workflows/executor.test.ts` | 51 |
| Workflow Definitions | `packages/core/src/workflows/definitions.test.ts` | 62 |
| File Watcher Store | `packages/stores/src/fileWatcher.test.ts` | 40 |
| Shortcuts Store | `packages/stores/src/shortcuts.test.ts` | 30 |
| Toast Store | `packages/stores/src/toast.test.ts` | 33 |
| Recovery Store | `packages/stores/src/recovery.test.ts` | 40 |
| **Total** | | **274** |

---

## Part 4: Architectural Decisions

### Decision 1: Where should workflow definitions live?

**Decision:** Bundle built-in workflows with app, support user-defined workflows in `~/.midlight/workflows/`

**Rationale:**
- Built-in workflows ensure consistent experience
- User directory allows customization without app updates
- Simple JSON format is approachable for power users

### Decision 2: How should context be assembled at the AI boundary?

**Decision:** Assembly in TypeScript (packages/stores/src/ai.ts), not Rust backend

**Rationale:**
- Flexibility for UI interactions (showing context, toggling layers)
- Easier to test and modify
- Context logic is business logic, not infrastructure

### Decision 3: Should project detection be automatic or explicit?

**Decision:** Automatic detection based on `.project.midlight` file presence

**Rationale:**
- Vision explicitly states "A folder becomes a project when it contains a .project.midlight file"
- Users don't need to think about project setup beyond creating the file
- Workflows handle project creation, making it seamless

### Decision 4: How should automatic context updates work without being intrusive?

**Decision:** Silent by default, opt-in notifications

**Rationale:**
- Vision states "This happens automatically by default"
- Power users can enable "ask before saving"
- Notifications are off by default to reduce interruption
- All updates are visible in the context document (editable)

---

## Appendix A: File Changes Summary

### Files Created (Phases 1-5)

| File | Purpose |
|------|---------|
| `packages/core/src/context/parser.ts` | Parse context.midlight sections |
| `packages/core/src/context/parser.test.ts` | Parser unit tests |
| `packages/core/src/context/updater.ts` | Context update logic |
| `packages/core/src/context/updater.test.ts` | Updater unit tests |
| `packages/core/src/workflows/types.ts` | Workflow type definitions |
| `packages/core/src/workflows/executor.ts` | Workflow execution engine |
| `packages/core/src/workflows/executor.test.ts` | Executor unit tests |
| `packages/core/src/workflows/definitions.ts` | Built-in workflows |
| `packages/core/src/workflows/definitions.test.ts` | Definitions unit tests |
| `packages/core/src/workflows/index.ts` | Workflow exports |
| `packages/stores/src/workflow.ts` | Workflow state store |
| `packages/stores/src/context.ts` | Context update state |
| `packages/stores/src/project.ts` | Project state store |
| `apps/desktop/src/lib/components/WorkflowWizard.svelte` | Interview UI |
| `apps/desktop/src/lib/components/WorkflowPicker.svelte` | Selection UI |
| `apps/desktop/src/lib/components/IndexStatusBadge.svelte` | RAG index status indicator |
| `apps/desktop/src/lib/components/ArchivedProjectsSection.svelte` | Archived projects list |

### Files Modified (Phases 1-5)

| File | Changes |
|------|---------|
| `packages/core/src/index.ts` | Export context and workflow modules |
| `packages/stores/src/index.ts` | Export new stores and types |
| `packages/stores/src/ai.ts` | Layered context, fresh start mode, semantic layer type |
| `packages/stores/src/settings.ts` | New context settings fields |
| `apps/desktop/src-tauri/src/commands/workspace.rs` | Project scanning commands |
| `apps/desktop/src-tauri/src/lib.rs` | Register new Tauri commands |
| `apps/desktop/src/App.svelte` | Workflow initialization, RAG auto-indexing |
| `apps/desktop/src/lib/components/Sidebar.svelte` | "New Project...", IndexStatusBadge, ArchivedProjectsSection |
| `apps/desktop/src/lib/components/FileContextMenu.svelte` | RAG indexing menu options |
| `apps/desktop/src/lib/components/Chat/ContextPicker.svelte` | Semantic search integration |
| `apps/desktop/src/lib/components/ContextPanel.svelte` | Token budget, layer toggles |

---

## Appendix B: Type Definitions

```typescript
// Complete type definitions for reference

// Project Types
interface ProjectConfig {
  version: 1;
  name: string;
  icon?: string;
  color?: string;
  status: ProjectStatus;
  createdAt: string;
  workflowSource?: string;
  context: ProjectContextSettings;
}

type ProjectStatus = 'active' | 'paused' | 'archived';

interface ProjectContextSettings {
  includeGlobalContext: boolean;
  autoUpdateContext: boolean;
  askBeforeUpdating: boolean;
}

// Context Types
interface ContextLayer {
  type: ContextLayerType;
  enabled: boolean;
  content: string;
  source: string;
  priority: number;
  tokenCount?: number;
}

type ContextLayerType = 'global' | 'project' | 'document' | 'mentioned' | 'selection' | 'semantic';

interface ContextDocument {
  overview: string;
  currentStatus: string;
  keyDecisions: KeyDecision[];
  openQuestions: OpenQuestion[];
  aiNotes: string;
}

interface KeyDecision {
  date: string;
  description: string;
}

interface OpenQuestion {
  text: string;
  resolved: boolean;
}

// Workflow Types
interface WorkflowDefinition {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  interview: InterviewStep[];
  templates: TemplateDefinition[];
  contextSections: ContextSectionTemplates;
  projectNameTemplate?: string;
  projectColor?: string;
}

interface InterviewStep {
  id: string;
  question: string;
  type: InterviewStepType;
  options?: string[];
  required: boolean;
  placeholder?: string;
  helpText?: string;
  defaultValue?: string | number | string[];
  validation?: string;
  validationMessage?: string;
  showIf?: {
    stepId: string;
    equals?: string | string[];
    notEquals?: string | string[];
  };
}

type InterviewStepType = 'text' | 'number' | 'select' | 'multiselect' | 'date' | 'textarea';

interface TemplateDefinition {
  path: string;
  name: string;
  type: 'file' | 'folder';
  contentTemplate?: string;
  generateWithLLM?: boolean;
  llmPrompt?: string;
  openAfterCreate?: boolean;
}

interface ContextSectionTemplates {
  overview: string;
  aiNotes: string;
  initialStatus?: string;
  initialDecisions?: string[];
  initialQuestions?: string[];
}

interface WorkflowState {
  phase: WorkflowPhase;
  availableWorkflows: WorkflowDefinition[];
  activeWorkflow: WorkflowDefinition | null;
  currentStepIndex: number;
  answers: WorkflowAnswers;
  validationErrors: Record<string, string>;
  executionProgress: WorkflowExecutionProgress | null;
  executionResult: WorkflowExecutionResult | null;
  error: string | null;
  parentPath: string | null;
}

type WorkflowPhase = 'idle' | 'selecting' | 'interview' | 'executing' | 'complete' | 'error';

type WorkflowAnswers = Record<string, string | number | string[] | undefined>;

interface WorkflowExecutionProgress {
  phase: 'creating-project' | 'creating-context' | 'creating-files' | 'generating-content' | 'complete';
  currentStep: number;
  totalSteps: number;
  currentFile?: string;
  percentComplete: number;
}

interface WorkflowExecutionResult {
  success: boolean;
  projectPath?: string;
  error?: string;
  createdFiles: string[];
  failedFiles: { path: string; error: string }[];
}
```

---

*Document prepared for the Midlight development team. Phases 1-5 are complete with 274 passing tests. Only backend Rust optimizations (project scan caching, incremental RAG indexing) remain.*
