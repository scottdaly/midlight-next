// @midlight/stores - Svelte stores for Midlight state management

export {
  fileSystem,
  activeFile,
  activeFileIndex,
  isSaving,
  hasPendingDiffs,
  selectedPaths,
  selectionCount,
  hasClipboard,
  clipboardOperation,
  pendingNewItem,
  stagedEdit,
  hasStagedEdit,
} from './fileSystem.js';
export type { PendingNewItem, StagedEdit } from './fileSystem.js';

// Diff utilities
export { computeWordDiff, mergeConsecutiveSegments, createDiffContent, createMergedDiffDocument } from './utils/diff.js';
export type { DiffSegment } from './utils/diff.js';
export {
  ai,
  isStreaming,
  agentEnabled,
  activeConversation,
  inlineEditState,
  isInlineEditActive,
} from './ai.js';
export type { ToolExecutor, ContextItem, InlineEditState } from './ai.js';
export { versions, selectedVersion } from './versions.js';
export { auth, isAuthenticated, currentUser } from './auth.js';
export type { User, Subscription, Quota, AuthState } from './auth.js';
export { settings } from './settings.js';
export type { Theme, SettingsState } from './settings.js';
export { ui, rightPanelMode, isRightPanelOpen, leftSidebarOpen } from './ui.js';
export type { RightPanelMode, UIState } from './ui.js';
export { editor, editorReady } from './editor.js';
export {
  agent,
  isAgentRunning,
  currentExecution,
  pendingChanges,
  hasPendingChanges,
  recentExecutions,
} from './agent.js';
export type {
  ToolExecutionStatus,
  ToolExecution,
  ToolResult,
  PendingChange,
  AgentState,
} from './agent.js';
