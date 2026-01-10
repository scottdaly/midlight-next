// @midlight/stores/sync - Cloud sync state management

import { writable, derived, get } from 'svelte/store';
import { auth } from './auth.js';
import { network } from './network.js';

export interface SyncDocument {
  id: string;
  path: string;
  contentHash: string;
  sidecarHash: string;
  version: number;
  sizeBytes: number;
  updatedAt: string;
  deleted?: boolean;
}

export interface SyncConflict {
  id: string;
  documentId: string;
  path: string;
  localVersion: number;
  remoteVersion: number;
  createdAt: string;
}

export interface SyncUsage {
  documentCount: number;
  totalSizeBytes: number;
  limitBytes: number;
  percentUsed: number;
  lastSyncAt: string | null;
}

export interface LocalDocumentVersion {
  path: string;
  contentHash: string;
  sidecarHash: string;
  version: number;
  lastSyncedAt: string | null;
  pendingUpload: boolean;
}

export interface SyncState {
  // Whether sync is enabled for this user
  enabled: boolean;

  // Whether initial sync has completed
  initialized: boolean;

  // Remote document list (cached from server)
  remoteDocuments: SyncDocument[];

  // Local document versions (tracked locally)
  localVersions: Map<string, LocalDocumentVersion>;

  // Active conflicts awaiting resolution
  conflicts: SyncConflict[];

  // Currently active conflict for dialog
  activeConflict: SyncConflict | null;

  // Storage usage
  usage: SyncUsage | null;

  // Sync status
  isSyncing: boolean;
  lastSyncAt: Date | null;
  lastSyncError: string | null;

  // Auto-sync settings
  autoSyncEnabled: boolean;
  autoSyncIntervalMs: number;
}

const defaultState: SyncState = {
  enabled: false,
  initialized: false,
  remoteDocuments: [],
  localVersions: new Map(),
  conflicts: [],
  activeConflict: null,
  usage: null,
  isSyncing: false,
  lastSyncAt: null,
  lastSyncError: null,
  autoSyncEnabled: true,
  autoSyncIntervalMs: 30000, // 30 seconds
};

function createSyncStore() {
  const { subscribe, set, update } = writable<SyncState>(defaultState);

  let autoSyncInterval: ReturnType<typeof setInterval> | null = null;
  let syncCallback: (() => Promise<void>) | null = null;

  return {
    subscribe,

    /**
     * Enable sync for authenticated users
     */
    enable() {
      update((s) => ({ ...s, enabled: true }));
    },

    /**
     * Disable sync
     */
    disable() {
      this.stopAutoSync();
      update((s) => ({
        ...s,
        enabled: false,
        initialized: false,
        remoteDocuments: [],
        localVersions: new Map(),
        conflicts: [],
        usage: null,
      }));
    },

    /**
     * Set the remote documents from server
     */
    setRemoteDocuments(documents: SyncDocument[]) {
      update((s) => ({ ...s, remoteDocuments: documents }));
    },

    /**
     * Update a single remote document
     */
    updateRemoteDocument(document: SyncDocument) {
      update((s) => {
        const index = s.remoteDocuments.findIndex((d) => d.id === document.id);
        const remoteDocuments = [...s.remoteDocuments];
        if (index >= 0) {
          remoteDocuments[index] = document;
        } else {
          remoteDocuments.push(document);
        }
        return { ...s, remoteDocuments };
      });
    },

    /**
     * Remove a remote document
     */
    removeRemoteDocument(documentId: string) {
      update((s) => ({
        ...s,
        remoteDocuments: s.remoteDocuments.filter((d) => d.id !== documentId),
      }));
    },

    /**
     * Set local document version
     */
    setLocalVersion(path: string, version: LocalDocumentVersion) {
      update((s) => {
        const localVersions = new Map(s.localVersions);
        localVersions.set(path, version);
        return { ...s, localVersions };
      });
    },

    /**
     * Mark a document as pending upload
     */
    markPendingUpload(path: string, pending: boolean) {
      update((s) => {
        const localVersions = new Map(s.localVersions);
        const existing = localVersions.get(path);
        if (existing) {
          localVersions.set(path, { ...existing, pendingUpload: pending });
        }
        return { ...s, localVersions };
      });
    },

    /**
     * Set conflicts
     */
    setConflicts(conflicts: SyncConflict[]) {
      update((s) => ({ ...s, conflicts }));
    },

    /**
     * Add a conflict
     */
    addConflict(conflict: SyncConflict) {
      update((s) => ({
        ...s,
        conflicts: [...s.conflicts, conflict],
      }));
    },

    /**
     * Remove a conflict (after resolution)
     */
    removeConflict(conflictId: string) {
      update((s) => ({
        ...s,
        conflicts: s.conflicts.filter((c) => c.id !== conflictId),
        activeConflict: s.activeConflict?.id === conflictId ? null : s.activeConflict,
      }));
    },

    /**
     * Set active conflict for dialog
     */
    setActiveConflict(conflict: SyncConflict | null) {
      update((s) => ({ ...s, activeConflict: conflict }));
    },

    /**
     * Set usage stats
     */
    setUsage(usage: SyncUsage | null) {
      update((s) => ({ ...s, usage }));
    },

    /**
     * Set syncing state
     */
    setSyncing(isSyncing: boolean) {
      update((s) => ({ ...s, isSyncing }));
      network.setSyncing(isSyncing);
    },

    /**
     * Record successful sync
     */
    recordSync() {
      const now = new Date();
      update((s) => ({
        ...s,
        lastSyncAt: now,
        lastSyncError: null,
        initialized: true,
      }));
    },

    /**
     * Record sync error
     */
    recordSyncError(error: string) {
      update((s) => ({ ...s, lastSyncError: error }));
      network.setSyncError(error);
    },

    /**
     * Register the sync callback (called by auto-sync)
     */
    registerSyncCallback(callback: () => Promise<void>) {
      syncCallback = callback;
    },

    /**
     * Start auto-sync
     */
    startAutoSync() {
      this.stopAutoSync();

      const state = get({ subscribe });
      if (!state.autoSyncEnabled || !state.enabled) return;

      autoSyncInterval = setInterval(async () => {
        const currentState = get({ subscribe });
        const authState = get(auth);
        const networkState = get(network);

        // Only sync if conditions are right
        if (
          currentState.enabled &&
          authState.isAuthenticated &&
          networkState.online &&
          !currentState.isSyncing &&
          syncCallback
        ) {
          try {
            await syncCallback();
          } catch (error) {
            console.error('[Sync] Auto-sync error:', error);
          }
        }
      }, state.autoSyncIntervalMs);
    },

    /**
     * Stop auto-sync
     */
    stopAutoSync() {
      if (autoSyncInterval) {
        clearInterval(autoSyncInterval);
        autoSyncInterval = null;
      }
    },

    /**
     * Set auto-sync enabled
     */
    setAutoSyncEnabled(enabled: boolean) {
      update((s) => ({ ...s, autoSyncEnabled: enabled }));
      if (enabled) {
        this.startAutoSync();
      } else {
        this.stopAutoSync();
      }
    },

    /**
     * Set auto-sync interval
     */
    setAutoSyncInterval(intervalMs: number) {
      update((s) => ({ ...s, autoSyncIntervalMs: intervalMs }));
      // Restart auto-sync with new interval
      const state = get({ subscribe });
      if (state.autoSyncEnabled) {
        this.startAutoSync();
      }
    },

    /**
     * Get document by path
     */
    getDocumentByPath(path: string): SyncDocument | undefined {
      const state = get({ subscribe });
      return state.remoteDocuments.find((d) => d.path === path && !d.deleted);
    },

    /**
     * Check if path has pending changes
     */
    hasPendingChanges(path: string): boolean {
      const state = get({ subscribe });
      const local = state.localVersions.get(path);
      return local?.pendingUpload ?? false;
    },

    /**
     * Reset the store
     */
    reset() {
      this.stopAutoSync();
      syncCallback = null;
      set(defaultState);
    },
  };
}

export const sync = createSyncStore();

// Derived stores
export const isSyncEnabled = derived(sync, ($sync) => $sync.enabled);
export const isSyncing = derived(sync, ($sync) => $sync.isSyncing);
export const hasConflicts = derived(sync, ($sync) => $sync.conflicts.length > 0);
export const conflictCount = derived(sync, ($sync) => $sync.conflicts.length);
export const syncUsage = derived(sync, ($sync) => $sync.usage);
export const lastSyncTime = derived(sync, ($sync) => $sync.lastSyncAt);
export const syncError = derived(sync, ($sync) => $sync.lastSyncError);

// Combined sync status for UI
export const syncStatus = derived(
  [sync, network],
  ([$sync, $network]) => {
    if (!$sync.enabled) return 'disabled';
    if (!$network.online) return 'offline';
    if ($sync.isSyncing) return 'syncing';
    if ($sync.conflicts.length > 0) return 'conflict';
    if ($sync.lastSyncError) return 'error';
    if (!$sync.initialized) return 'initializing';
    return 'synced';
  }
);

export type SyncStatusType = 'disabled' | 'offline' | 'syncing' | 'conflict' | 'error' | 'initializing' | 'synced';
