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
export {
  subscription,
  isFreeTier,
  quotaPercentUsed,
  isQuotaExceeded,
  showQuotaWarning,
  quotaWarningSeverity,
  quotaDisplay,
} from './subscription.js';
export type { SubscriptionStatus, QuotaInfo, Price, SubscriptionState } from './subscription.js';
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
export {
  importStore,
  isImporting,
  currentImport,
  importProgress,
  exportStore,
  isExporting,
  exportProgress,
  exportError,
} from './import.js';
export type {
  ImportSourceType,
  ImportPhase,
  ImportStep,
  ImportErrorInfo,
  ImportWarningInfo,
  ImportProgress,
  ImportResult,
  CurrentImport,
  ImportState,
  ExportType,
  ExportProgress,
  ExportState,
} from './import.js';
export {
  recoveryStore,
  hasPendingRecoveries,
  showRecoveryDialog,
  pendingRecoveries,
  isCheckingRecovery,
  scheduleWalWrite,
  cancelWalWrite,
  flushWalWrite,
  clearAllWalWrites,
} from './recovery.js';
export type { RecoveryFile, RecoveryDecision, RecoveryState } from './recovery.js';
export {
  toastStore,
  toasts,
  visibleToasts,
  hiddenToastCount,
  hasToasts,
} from './toast.js';
export type { ToastType, ToastAction, Toast, ToastState } from './toast.js';
export {
  fileWatcherStore,
  hasPendingExternalChanges,
  pendingChangeCount,
  showExternalChangeDialog,
  pendingExternalChanges,
  selectedExternalChange,
  isFileWatching,
  changesByType,
} from './fileWatcher.js';
export type { ChangeType, ExternalChange, FileWatcherState } from './fileWatcher.js';
export {
  shortcuts,
  allShortcuts,
  shortcutsByCategory,
  hasCustomizations,
  getModifierName,
  getDisplayKey,
  matchesShortcut,
} from './shortcuts.js';
export type { Shortcut, ShortcutCategory, ShortcutState } from './shortcuts.js';
export {
  updateStore,
  hasUpdate,
  showUpdateDialog,
  updateStatus,
  availableUpdate,
  downloadProgress,
  isChecking,
  isDownloading,
  isReadyToInstall,
  updateError,
} from './updates.js';
export type { UpdateInfo, UpdateProgress, UpdateStatus, UpdateState } from './updates.js';
export {
  windowStateStore,
  windowState,
  windowStateLoaded,
} from './windowState.js';
export type { WindowState } from './windowState.js';
