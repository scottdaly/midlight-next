// @midlight/stores/storage - Storage usage monitoring

import { writable } from 'svelte/store';
import {
  getStorageUsageInfo,
  requestPersistentStorage,
  isStoragePersisted,
  type StorageUsageInfo,
} from '@midlight/core/storage';
import { formatBytes } from '@midlight/core/utils';

export interface StorageState {
  // Whether storage info is available
  isSupported: boolean;

  // Whether storage is being refreshed
  isLoading: boolean;

  // Storage type being used
  storageType: 'opfs' | 'indexeddb' | 'unknown';

  // Current usage information
  usage: StorageUsageInfo | null;

  // Whether storage is persisted (won't be cleared by browser)
  isPersisted: boolean;

  // Last time storage was checked
  lastCheckedAt: Date | null;

  // Error if storage check failed
  error: string | null;
}

const defaultState: StorageState = {
  isSupported: false,
  isLoading: false,
  storageType: 'unknown',
  usage: null,
  isPersisted: false,
  lastCheckedAt: null,
  error: null,
};

function createStorageStore() {
  const { subscribe, set, update } = writable<StorageState>(defaultState);

  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  return {
    subscribe,

    /**
     * Initialize storage monitoring
     */
    async init(storageType: 'opfs' | 'indexeddb') {
      update((s) => ({
        ...s,
        isSupported: true,
        storageType,
        isLoading: true,
      }));

      try {
        // Check if storage is persisted
        const persisted = await isStoragePersisted();

        // Get initial usage
        const usage = await getStorageUsageInfo();

        update((s) => ({
          ...s,
          isLoading: false,
          isPersisted: persisted,
          usage,
          lastCheckedAt: new Date(),
          error: null,
        }));
      } catch (error) {
        update((s) => ({
          ...s,
          isLoading: false,
          error: error instanceof Error ? error.message : 'Failed to check storage',
        }));
      }
    },

    /**
     * Refresh storage usage information
     */
    async refresh() {
      update((s) => ({ ...s, isLoading: true }));

      try {
        const usage = await getStorageUsageInfo();
        const persisted = await isStoragePersisted();

        update((s) => ({
          ...s,
          isLoading: false,
          usage,
          isPersisted: persisted,
          lastCheckedAt: new Date(),
          error: null,
        }));
      } catch (error) {
        update((s) => ({
          ...s,
          isLoading: false,
          error: error instanceof Error ? error.message : 'Failed to check storage',
        }));
      }
    },

    /**
     * Request persistent storage (prevents browser from clearing data)
     */
    async requestPersistence(): Promise<boolean> {
      try {
        const granted = await requestPersistentStorage();
        update((s) => ({ ...s, isPersisted: granted }));
        return granted;
      } catch {
        return false;
      }
    },

    /**
     * Start automatic refresh of storage usage
     */
    startAutoRefresh(intervalMs = 60000) {
      this.stopAutoRefresh();
      refreshTimer = setInterval(() => {
        this.refresh();
      }, intervalMs);
    },

    /**
     * Stop automatic refresh
     */
    stopAutoRefresh() {
      if (refreshTimer) {
        clearInterval(refreshTimer);
        refreshTimer = null;
      }
    },

    /**
     * Reset the store
     */
    reset() {
      this.stopAutoRefresh();
      set(defaultState);
    },
  };
}

export const storage = createStorageStore();

// Re-export utility for formatting
export { formatBytes };
