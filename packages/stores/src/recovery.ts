// @midlight/stores/recovery - Crash recovery state management

import { writable, derived } from 'svelte/store';

// ============================================================================
// Types
// ============================================================================

export interface RecoveryFile {
  fileKey: string;
  walContent: string;
  /** ISO 8601 timestamp string */
  walTime: string;
  workspaceRoot: string;
}

export interface RecoveryDecision {
  fileKey: string;
  action: 'recover' | 'discard';
}

export interface RecoveryState {
  /** Recovery files found on startup */
  pendingRecoveries: RecoveryFile[];
  /** Whether the recovery dialog should be shown */
  showDialog: boolean;
  /** Whether recovery check is in progress */
  isChecking: boolean;
  /** Currently active WAL writers (file keys with pending writes) */
  activeWalFiles: Set<string>;
  /** Last error message */
  error: string | null;
}

// ============================================================================
// WAL Debouncing
// ============================================================================

interface DebouncedWrite {
  timeout: ReturnType<typeof setTimeout>;
  lastContent: string;
}

// Track debounced writes per file
const debouncedWrites = new Map<string, DebouncedWrite>();

// Default debounce delay (2 seconds as per architecture doc)
const WAL_DEBOUNCE_MS = 2000;

// ============================================================================
// Recovery Store
// ============================================================================

const initialState: RecoveryState = {
  pendingRecoveries: [],
  showDialog: false,
  isChecking: false,
  activeWalFiles: new Set(),
  error: null,
};

function createRecoveryStore() {
  const { subscribe, set, update } = writable<RecoveryState>(initialState);

  return {
    subscribe,

    /**
     * Start checking for recovery files
     */
    startCheck() {
      update((s) => ({
        ...s,
        isChecking: true,
        error: null,
      }));
    },

    /**
     * Set pending recovery files after check completes
     */
    setPendingRecoveries(files: RecoveryFile[]) {
      update((s) => ({
        ...s,
        isChecking: false,
        pendingRecoveries: files,
        showDialog: files.length > 0,
      }));
    },

    /**
     * Complete recovery check with error
     */
    checkFailed(error: string) {
      update((s) => ({
        ...s,
        isChecking: false,
        error,
      }));
    },

    /**
     * Remove a recovery file from pending list after user decision
     */
    removeRecovery(fileKey: string) {
      update((s) => ({
        ...s,
        pendingRecoveries: s.pendingRecoveries.filter((r) => r.fileKey !== fileKey),
        showDialog: s.pendingRecoveries.filter((r) => r.fileKey !== fileKey).length > 0,
      }));
    },

    /**
     * Clear all pending recoveries (after user dismisses dialog)
     */
    clearPendingRecoveries() {
      update((s) => ({
        ...s,
        pendingRecoveries: [],
        showDialog: false,
      }));
    },

    /**
     * Close the recovery dialog
     */
    closeDialog() {
      update((s) => ({
        ...s,
        showDialog: false,
      }));
    },

    /**
     * Open the recovery dialog (if there are pending recoveries)
     */
    openDialog() {
      update((s) => ({
        ...s,
        showDialog: s.pendingRecoveries.length > 0,
      }));
    },

    /**
     * Mark a file as having active WAL writing
     */
    addActiveWal(fileKey: string) {
      update((s) => {
        const newSet = new Set(s.activeWalFiles);
        newSet.add(fileKey);
        return {
          ...s,
          activeWalFiles: newSet,
        };
      });
    },

    /**
     * Mark a file as no longer having active WAL writing
     */
    removeActiveWal(fileKey: string) {
      update((s) => {
        const newSet = new Set(s.activeWalFiles);
        newSet.delete(fileKey);
        return {
          ...s,
          activeWalFiles: newSet,
        };
      });
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
     * Reset the store
     */
    reset() {
      // Clear any pending debounced writes
      for (const [, write] of debouncedWrites) {
        clearTimeout(write.timeout);
      }
      debouncedWrites.clear();
      set(initialState);
    },
  };
}

export const recoveryStore = createRecoveryStore();

// ============================================================================
// Derived Stores
// ============================================================================

/** Whether there are pending recoveries to handle */
export const hasPendingRecoveries = derived(
  recoveryStore,
  ($store) => $store.pendingRecoveries.length > 0
);

/** Whether the recovery dialog should be shown */
export const showRecoveryDialog = derived(
  recoveryStore,
  ($store) => $store.showDialog
);

/** Current pending recoveries */
export const pendingRecoveries = derived(
  recoveryStore,
  ($store) => $store.pendingRecoveries
);

/** Whether recovery check is in progress */
export const isCheckingRecovery = derived(
  recoveryStore,
  ($store) => $store.isChecking
);

// ============================================================================
// WAL Write Utilities
// ============================================================================

/**
 * Debounced WAL write function
 * Waits for idle period before writing to reduce disk I/O
 */
export function scheduleWalWrite(
  fileKey: string,
  content: string,
  writeCallback: (fileKey: string, content: string) => Promise<void>
): void {
  // Check if content changed
  const existing = debouncedWrites.get(fileKey);
  if (existing?.lastContent === content) {
    return; // No change, skip
  }

  // Clear existing timeout
  if (existing) {
    clearTimeout(existing.timeout);
  }

  // Schedule new write
  const timeout = setTimeout(async () => {
    try {
      await writeCallback(fileKey, content);
      debouncedWrites.delete(fileKey);
    } catch (error) {
      console.error(`WAL write failed for ${fileKey}:`, error);
    }
  }, WAL_DEBOUNCE_MS);

  debouncedWrites.set(fileKey, { timeout, lastContent: content });
  recoveryStore.addActiveWal(fileKey);
}

/**
 * Cancel any pending WAL write for a file
 */
export function cancelWalWrite(fileKey: string): void {
  const existing = debouncedWrites.get(fileKey);
  if (existing) {
    clearTimeout(existing.timeout);
    debouncedWrites.delete(fileKey);
    recoveryStore.removeActiveWal(fileKey);
  }
}

/**
 * Flush WAL write immediately (before save)
 */
export async function flushWalWrite(
  fileKey: string,
  writeCallback: (fileKey: string, content: string) => Promise<void>
): Promise<void> {
  const existing = debouncedWrites.get(fileKey);
  if (existing) {
    clearTimeout(existing.timeout);
    try {
      await writeCallback(fileKey, existing.lastContent);
    } finally {
      debouncedWrites.delete(fileKey);
      recoveryStore.removeActiveWal(fileKey);
    }
  }
}

/**
 * Clear all pending WAL writes (on workspace close or app quit)
 */
export function clearAllWalWrites(): void {
  for (const [, write] of debouncedWrites) {
    clearTimeout(write.timeout);
  }
  debouncedWrites.clear();
  recoveryStore.reset();
}
