# Phase 4: AI Integration - Implementation Plan

## Executive Summary

This document provides a detailed implementation plan for integrating AI capabilities into the Midlight desktop app (Tauri + Svelte 5) and web version.

**Existing Infrastructure:**
- `ChatPanel.svelte` with basic chat UI
- `@midlight/stores/ai.ts` with conversation management
- Backend API at `midlight.ai/api/llm` (already operational)

**Total Effort:** 22-31 days (4-6 weeks)

---

## Architecture Overview

```
+------------------+     +------------------+     +------------------+
|  Svelte Frontend |     |  Tauri Backend   |     |  midlight.ai     |
|  (ChatPanel,     | --> |  (Rust commands) | --> |  /api/llm/*      |
|   AI Store)      |     |  or direct fetch |     |  (LLM Proxy)     |
+------------------+     +------------------+     +------------------+
        |                        |
        v                        v
+------------------+     +------------------+
|  Document Store  |     |  Workspace Mgr   |
|  (fileSystem)    |     |  (checkpoints)   |
+------------------+     +------------------+
```

### Key Architecture Decisions

1. **Platform Abstraction**: `LLMClient` interface works across both Tauri (desktop) and web
2. **Streaming via SSE**: Server-Sent Events for streaming responses
3. **Agent Tools via Backend**: Tool execution in Tauri/backend for desktop; via API for web
4. **Shared Stores**: All AI state in `@midlight/stores/ai.ts`

---

## Sub-Phases

### 4.1: LLM Service Foundation (4-6 days)

**Goal**: Establish core LLM connectivity with streaming support

#### New Files

| File | Purpose |
|------|---------|
| `packages/core/src/llm/types.ts` | LLM type definitions |
| `packages/core/src/llm/webClient.ts` | Web-based LLM client using fetch/SSE |
| `apps/desktop/src-tauri/src/commands/llm.rs` | Tauri LLM commands |
| `apps/desktop/src-tauri/src/services/llm_service.rs` | Rust HTTP client for API |
| `apps/desktop/src/lib/llm.ts` | Tauri LLM adapter |

#### Tasks

- [ ] Create LLM type definitions (ChatMessage, ChatOptions, StreamChunk, etc.)
- [ ] Create WebLLMClient with fetch-based implementation
- [ ] Create Tauri commands: `llm_chat`, `llm_chat_stream`, `llm_chat_with_tools`
- [ ] Create TauriLLMClient adapter using invoke
- [ ] Extend AI store with `sendMessage()` implementation
- [ ] Add auth token handling for API requests

#### Testing Checkpoint
- Send non-streaming message and receive response
- Send streaming message and see chunks arrive
- Error handling for auth failures and quota exceeded

---

### 4.2: Enhanced Chat UI (3-4 days)

**Goal**: Upgrade ChatPanel with streaming, message history, and proper UX

#### New Files

| File | Purpose |
|------|---------|
| `components/Chat/MessageBubble.svelte` | Individual message display |
| `components/Chat/ToolActionCard.svelte` | Tool execution display |
| `components/Chat/ThinkingSteps.svelte` | AI reasoning display |
| `components/Chat/ConversationTabs.svelte` | Multi-conversation tabs |
| `components/common/Markdown.svelte` | Markdown rendering |

#### Tasks

- [ ] Create MessageBubble with markdown support
- [ ] Create ToolActionCard for tool calls
- [ ] Create ThinkingSteps for reasoning display
- [ ] Create ConversationTabs for multiple chats
- [ ] Refactor ChatPanel to use new components
- [ ] Add streaming text animation
- [ ] Implement auto-scroll with user override
- [ ] Add model selector dropdown

#### Testing Checkpoint
- Messages display with proper formatting
- Streaming text appears progressively
- Conversation tabs work correctly

---

### 4.3: Context Picker (@ Mentions) (2-3 days)

**Goal**: Allow users to reference files in their prompts

#### New Files

| File | Purpose |
|------|---------|
| `components/Chat/ContextPicker.svelte` | File autocomplete dropdown |
| `components/Chat/ContextPills.svelte` | Context summary display |
| `components/Chat/ChatInput.svelte` | Enhanced input with @ detection |

#### Tasks

- [ ] Create ContextPicker with file search/filter
- [ ] Create ContextPills showing selected files
- [ ] Create ChatInput with @ trigger detection
- [ ] Extend AI store with context management
- [ ] Add file content loading for @ mentions
- [ ] Implement keyboard navigation in picker

#### Testing Checkpoint
- Typing @ shows file picker
- Arrow keys navigate, Enter selects
- Selected files appear as chips
- Context included in API requests

---

### 4.4: AI Agent Executor (5-7 days)

**Goal**: Enable AI to execute document operations

#### Agent Tools

| Tool | Description | Destructive |
|------|-------------|-------------|
| `list_documents` | List files in folder | No |
| `read_document` | Read document content | No |
| `create_document` | Create new document | No |
| `edit_document` | Edit existing document | No |
| `move_document` | Move/rename document | No |
| `delete_document` | Delete document | Yes |
| `search_documents` | Search content | No |

#### New Files

| File | Purpose |
|------|---------|
| `packages/core/src/agent/tools.ts` | Tool definitions |
| `packages/stores/src/agent.ts` | Agent execution state |
| `src-tauri/src/commands/agent.rs` | Tauri agent commands |
| `src-tauri/src/services/agent_executor.rs` | Tool execution logic |

#### Tasks

- [ ] Create tool definitions with JSON schemas
- [ ] Create Tauri commands for agent operations
- [ ] Implement all 7 document tools in Rust
- [ ] Create agent store with status tracking
- [ ] Add agent loop to AI store
- [ ] Add tool execution progress UI

#### Testing Checkpoint
- "Create a document called Test" creates document
- "List my documents" shows file list
- "Edit Test.md to add a header" edits correctly
- Tool actions appear in chat UI

---

### 4.5: Pending Changes Review (3-4 days)

**Goal**: Show diffs and allow user to accept/reject AI changes

#### New Files

| File | Purpose |
|------|---------|
| `components/Chat/PendingChangesPanel.svelte` | Changes review UI |
| `components/common/DiffDisplay.svelte` | Diff visualization |

#### Tasks

- [ ] Create PendingChangesPanel with diff view
- [ ] Create DiffDisplay (unified/split modes)
- [ ] Add undo functionality via checkpoints
- [ ] Connect to checkpoint restoration
- [ ] Add visual indicators in file tree

#### Testing Checkpoint
- After AI edits, pending changes panel appears
- Diff shows before/after correctly
- Accept applies change permanently
- Reject restores original content

---

### 4.6: Inline Editing Mode (3-4 days)

**Goal**: AI suggestions shown directly in the document

#### New Files

| File | Purpose |
|------|---------|
| `components/Editor/InlineEditPrompt.svelte` | Floating prompt input |
| `components/Editor/InlineDiff.svelte` | Inline diff display |

#### Tasks

- [ ] Create InlineEditPrompt (Cmd+K trigger)
- [ ] Create InlineDiff with Accept/Reject
- [ ] Extend AI store for inline mode
- [ ] Add keyboard shortcut in Editor
- [ ] Implement selection-to-prompt flow

#### Testing Checkpoint
- Select text, press Cmd+K, see prompt
- Enter instruction, see AI response inline
- Accept replaces selection with result
- Cancel restores original

---

### 4.7: AI Annotations (2-3 days)

**Goal**: Visual markers showing where AI made edits

#### New Files

| File | Purpose |
|------|---------|
| `extensions/AIAnnotation.ts` | Tiptap annotation mark |
| `components/Editor/AnnotationGutter.svelte` | Margin icons |
| `components/Editor/AnnotationPopover.svelte` | Edit details popover |

#### Tasks

- [ ] Create AI annotation Tiptap extension
- [ ] Create annotation gutter component
- [ ] Create annotation popover
- [ ] Add toggle in toolbar
- [ ] Integrate with agent edits

#### Testing Checkpoint
- AI edits create annotations
- Annotations visible as margin icons
- Hovering shows edit details
- Toggle hides all annotations

---

## Dependency Graph

```
4.1 LLM Service Foundation
         |
         v
4.2 Enhanced Chat UI  <----+
         |                 |
         v                 |
4.3 Context Picker --------+
         |
         v
4.4 AI Agent Executor
         |
    +----+----+
    |         |
    v         v
4.5 Pending   4.6 Inline
    Changes       Editing
    Review
         |         |
         +----+----+
              |
              v
        4.7 AI Annotations
```

---

## Effort Summary

| Sub-Phase | Days | Dependencies |
|-----------|------|--------------|
| 4.1 LLM Service Foundation | 4-6 | Auth system |
| 4.2 Enhanced Chat UI | 3-4 | 4.1 |
| 4.3 Context Picker | 2-3 | 4.2 |
| 4.4 AI Agent Executor | 5-7 | 4.1, 4.3 |
| 4.5 Pending Changes Review | 3-4 | 4.4 |
| 4.6 Inline Editing | 3-4 | 4.1 |
| 4.7 AI Annotations | 2-3 | 4.4, 4.6 |
| **Total** | **22-31** | |

---

## Success Criteria

1. Users can chat with AI about their documents
2. AI can read, create, edit, move, and delete documents
3. All AI changes are reviewable before permanent application
4. Inline editing provides quick, contextual AI assistance
5. AI edit history is visible via annotations
6. Both desktop and web platforms share the same AI capabilities

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Streaming complexity | Start non-streaming, add incrementally |
| Agent corrupting docs | Pre-change checkpoints, validation |
| Auth token management | Centralize in auth store |
| Token limits | Truncate context, prioritize recent |
| Rate limiting | Retry with backoff, user feedback |

---

*Created: January 2025*
