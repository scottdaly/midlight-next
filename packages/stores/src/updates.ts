// @midlight/stores/updates - Auto-update state management

import { writable, derived } from 'svelte/store';

// ============================================================================
// Types
// ============================================================================

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  body?: string;
  date?: string;
}

export interface UpdateProgress {
  downloaded: number;
  total?: number;
}

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'ready'
  | 'error';

export interface UpdateState {
  status: UpdateStatus;
  updateInfo: UpdateInfo | null;
  progress: UpdateProgress | null;
  error: string | null;
  showDialog: boolean;
  lastChecked: number | null;
}

// ============================================================================
// Update Store
// ============================================================================

const initialState: UpdateState = {
  status: 'idle',
  updateInfo: null,
  progress: null,
  error: null,
  showDialog: false,
  lastChecked: null,
};

function createUpdateStore() {
  const { subscribe, set, update } = writable<UpdateState>(initialState);

  return {
    subscribe,

    /**
     * Start checking for updates
     */
    startCheck() {
      update((s) => ({
        ...s,
        status: 'checking',
        error: null,
      }));
    },

    /**
     * Update available - store info and optionally show dialog
     */
    setUpdateAvailable(info: UpdateInfo, showDialog = true) {
      update((s) => ({
        ...s,
        status: 'available',
        updateInfo: info,
        showDialog,
        lastChecked: Date.now(),
      }));
    },

    /**
     * No update available
     */
    setNoUpdate() {
      update((s) => ({
        ...s,
        status: 'idle',
        updateInfo: null,
        lastChecked: Date.now(),
      }));
    },

    /**
     * Start downloading update
     */
    startDownload() {
      update((s) => ({
        ...s,
        status: 'downloading',
        progress: { downloaded: 0 },
        error: null,
      }));
    },

    /**
     * Update download progress
     */
    setProgress(progress: UpdateProgress) {
      update((s) => ({
        ...s,
        progress,
      }));
    },

    /**
     * Download complete, ready to install
     */
    setReady() {
      update((s) => ({
        ...s,
        status: 'ready',
        progress: null,
      }));
    },

    /**
     * Set error state
     */
    setError(error: string) {
      update((s) => ({
        ...s,
        status: 'error',
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
        status: s.updateInfo ? 'available' : 'idle',
      }));
    },

    /**
     * Open the update dialog
     */
    openDialog() {
      update((s) => ({
        ...s,
        showDialog: true,
      }));
    },

    /**
     * Close the update dialog
     */
    closeDialog() {
      update((s) => ({
        ...s,
        showDialog: false,
      }));
    },

    /**
     * Dismiss update (user chose not to update now)
     */
    dismissUpdate() {
      update((s) => ({
        ...s,
        showDialog: false,
        // Keep the updateInfo so we can show a badge or reminder later
      }));
    },

    /**
     * Reset to initial state
     */
    reset() {
      set(initialState);
    },
  };
}

export const updateStore = createUpdateStore();

// ============================================================================
// Derived Stores
// ============================================================================

/**
 * Whether an update is available
 */
export const hasUpdate = derived(
  updateStore,
  ($state) => $state.updateInfo !== null && $state.status !== 'error'
);

/**
 * Whether the update dialog should be shown
 */
export const showUpdateDialog = derived(
  updateStore,
  ($state) => $state.showDialog
);

/**
 * Current update status
 */
export const updateStatus = derived(updateStore, ($state) => $state.status);

/**
 * Update info if available
 */
export const availableUpdate = derived(
  updateStore,
  ($state) => $state.updateInfo
);

/**
 * Download progress percentage (0-100)
 */
export const downloadProgress = derived(updateStore, ($state) => {
  if (!$state.progress) return 0;
  if (!$state.progress.total) return 0;
  return Math.round(($state.progress.downloaded / $state.progress.total) * 100);
});

/**
 * Whether currently checking for updates
 */
export const isChecking = derived(
  updateStore,
  ($state) => $state.status === 'checking'
);

/**
 * Whether currently downloading
 */
export const isDownloading = derived(
  updateStore,
  ($state) => $state.status === 'downloading'
);

/**
 * Whether update is ready to install
 */
export const isReadyToInstall = derived(
  updateStore,
  ($state) => $state.status === 'ready'
);

/**
 * Update error if any
 */
export const updateError = derived(updateStore, ($state) => $state.error);
