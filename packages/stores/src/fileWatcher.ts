// @midlight/stores/fileWatcher - External file change state management

import { writable, derived } from 'svelte/store';

// ============================================================================
// Types
// ============================================================================

export type ChangeType = 'modify' | 'create' | 'delete';

export interface ExternalChange {
  /** Relative path from workspace root */
  fileKey: string;
  /** Type of change detected */
  changeType: ChangeType;
  /** When the change was detected */
  timestamp: Date;
  /** User's decision (if made) */
  decision?: 'reload' | 'keep' | 'merge';
}

export interface FileWatcherState {
  /** Whether file watching is active */
  isWatching: boolean;
  /** Current workspace being watched */
  workspaceRoot: string | null;
  /** Pending external changes awaiting user decision */
  pendingChanges: ExternalChange[];
  /** Whether the external change dialog should be shown */
  showDialog: boolean;
  /** Currently selected change in dialog (for single-file review) */
  selectedChangeIndex: number;
  /** Error message if watching fails */
  error: string | null;
}

// ============================================================================
// File Watcher Store
// ============================================================================

const initialState: FileWatcherState = {
  isWatching: false,
  workspaceRoot: null,
  pendingChanges: [],
  showDialog: false,
  selectedChangeIndex: 0,
  error: null,
};

function createFileWatcherStore() {
  const { subscribe, set, update } = writable<FileWatcherState>(initialState);

  return {
    subscribe,

    /**
     * Set watching state when watcher starts
     */
    startWatching(workspaceRoot: string) {
      update((s) => ({
        ...s,
        isWatching: true,
        workspaceRoot,
        error: null,
      }));
    },

    /**
     * Clear watching state when watcher stops
     */
    stopWatching() {
      update((s) => ({
        ...s,
        isWatching: false,
        workspaceRoot: null,
        // Keep pending changes - user may still need to handle them
      }));
    },

    /**
     * Add a new external change
     */
    addChange(change: ExternalChange) {
      update((s) => {
        // Check if we already have a change for this file
        const existingIndex = s.pendingChanges.findIndex(
          (c) => c.fileKey === change.fileKey
        );

        let newChanges: ExternalChange[];
        if (existingIndex >= 0) {
          // Update existing change (escalate: modify -> delete becomes delete)
          newChanges = [...s.pendingChanges];
          const existing = newChanges[existingIndex];
          newChanges[existingIndex] = {
            ...existing,
            changeType: change.changeType === 'delete' ? 'delete' : existing.changeType,
            timestamp: change.timestamp,
            decision: undefined, // Reset decision on new change
          };
        } else {
          // Add new change
          newChanges = [...s.pendingChanges, change];
        }

        return {
          ...s,
          pendingChanges: newChanges,
          showDialog: newChanges.length > 0,
        };
      });
    },

    /**
     * Remove a change after handling
     */
    removeChange(fileKey: string) {
      update((s) => {
        const newChanges = s.pendingChanges.filter((c) => c.fileKey !== fileKey);
        return {
          ...s,
          pendingChanges: newChanges,
          showDialog: newChanges.length > 0,
          selectedChangeIndex: Math.min(s.selectedChangeIndex, Math.max(0, newChanges.length - 1)),
        };
      });
    },

    /**
     * Set decision for a change
     */
    setDecision(fileKey: string, decision: 'reload' | 'keep' | 'merge') {
      update((s) => ({
        ...s,
        pendingChanges: s.pendingChanges.map((c) =>
          c.fileKey === fileKey ? { ...c, decision } : c
        ),
      }));
    },

    /**
     * Clear all pending changes
     */
    clearAllChanges() {
      update((s) => ({
        ...s,
        pendingChanges: [],
        showDialog: false,
        selectedChangeIndex: 0,
      }));
    },

    /**
     * Close the dialog without clearing changes
     */
    closeDialog() {
      update((s) => ({
        ...s,
        showDialog: false,
      }));
    },

    /**
     * Open the dialog (if there are pending changes)
     */
    openDialog() {
      update((s) => ({
        ...s,
        showDialog: s.pendingChanges.length > 0,
      }));
    },

    /**
     * Select a change for review
     */
    selectChange(index: number) {
      update((s) => ({
        ...s,
        selectedChangeIndex: Math.max(0, Math.min(index, s.pendingChanges.length - 1)),
      }));
    },

    /**
     * Set error message
     */
    setError(error: string) {
      update((s) => ({
        ...s,
        error,
      }));
    },

    /**
     * Clear error
     */
    clearError() {
      update((s) => ({
        ...s,
        error: null,
      }));
    },

    /**
     * Reset to initial state
     */
    reset() {
      set(initialState);
    },

    /**
     * Check if a file has pending external changes
     */
    hasPendingChange(fileKey: string): boolean {
      let result = false;
      subscribe((s) => {
        result = s.pendingChanges.some((c) => c.fileKey === fileKey);
      })();
      return result;
    },
  };
}

export const fileWatcherStore = createFileWatcherStore();

// ============================================================================
// Derived Stores
// ============================================================================

/** Whether there are pending external changes */
export const hasPendingExternalChanges = derived(
  fileWatcherStore,
  ($store) => $store.pendingChanges.length > 0
);

/** Number of pending changes */
export const pendingChangeCount = derived(
  fileWatcherStore,
  ($store) => $store.pendingChanges.length
);

/** Whether the external change dialog should be shown */
export const showExternalChangeDialog = derived(
  fileWatcherStore,
  ($store) => $store.showDialog
);

/** Current pending changes */
export const pendingExternalChanges = derived(
  fileWatcherStore,
  ($store) => $store.pendingChanges
);

/** Currently selected change */
export const selectedExternalChange = derived(
  fileWatcherStore,
  ($store) => $store.pendingChanges[$store.selectedChangeIndex] ?? null
);

/** Whether file watching is active */
export const isFileWatching = derived(
  fileWatcherStore,
  ($store) => $store.isWatching
);

/** Changes grouped by type */
export const changesByType = derived(
  fileWatcherStore,
  ($store) => {
    const modified: ExternalChange[] = [];
    const created: ExternalChange[] = [];
    const deleted: ExternalChange[] = [];

    for (const change of $store.pendingChanges) {
      switch (change.changeType) {
        case 'modify':
          modified.push(change);
          break;
        case 'create':
          created.push(change);
          break;
        case 'delete':
          deleted.push(change);
          break;
      }
    }

    return { modified, created, deleted };
  }
);
