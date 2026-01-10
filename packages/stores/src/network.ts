// @midlight/stores/network - Network state management for offline support

import { writable, derived } from 'svelte/store';

export interface NetworkState {
  // Whether the browser reports being online
  online: boolean;

  // Whether we've confirmed connectivity to the server
  serverReachable: boolean;

  // Last time we successfully connected to the server
  lastOnlineAt: Date | null;

  // Number of pending sync operations
  pendingSyncCount: number;

  // Whether a sync is currently in progress
  isSyncing: boolean;

  // Last sync error, if any
  lastSyncError: string | null;

  // Connection type (wifi, cellular, etc.) if available
  connectionType: string | null;

  // Effective connection type (slow-2g, 2g, 3g, 4g) if available
  effectiveType: string | null;
}

const defaultState: NetworkState = {
  online: typeof navigator !== 'undefined' ? navigator.onLine : true,
  serverReachable: true,
  lastOnlineAt: null,
  pendingSyncCount: 0,
  isSyncing: false,
  lastSyncError: null,
  connectionType: null,
  effectiveType: null,
};

function createNetworkStore() {
  const { subscribe, set, update } = writable<NetworkState>(defaultState);

  // Track cleanup functions
  let cleanup: (() => void) | null = null;

  return {
    subscribe,

    /**
     * Initialize network monitoring
     * Should be called once when the app starts
     */
    init() {
      if (typeof window === 'undefined') return;

      // Online/offline event listeners
      const handleOnline = () => {
        update((s) => ({
          ...s,
          online: true,
          lastOnlineAt: new Date(),
        }));
      };

      const handleOffline = () => {
        update((s) => ({
          ...s,
          online: false,
          serverReachable: false,
        }));
      };

      window.addEventListener('online', handleOnline);
      window.addEventListener('offline', handleOffline);

      // Network Information API (if available)
      const connection = (navigator as NavigatorWithConnection).connection;
      if (connection) {
        const updateConnectionInfo = () => {
          update((s) => ({
            ...s,
            connectionType: connection.type || null,
            effectiveType: connection.effectiveType || null,
          }));
        };

        updateConnectionInfo();
        connection.addEventListener('change', updateConnectionInfo);

        cleanup = () => {
          window.removeEventListener('online', handleOnline);
          window.removeEventListener('offline', handleOffline);
          connection.removeEventListener('change', updateConnectionInfo);
        };
      } else {
        cleanup = () => {
          window.removeEventListener('online', handleOnline);
          window.removeEventListener('offline', handleOffline);
        };
      }

      // Listen for service worker messages
      if ('serviceWorker' in navigator) {
        navigator.serviceWorker.addEventListener('message', (event) => {
          if (event.data?.type === 'SYNC_READY') {
            // Service worker is ready to sync
            update((s) => ({ ...s, serverReachable: true }));
          }
        });
      }
    },

    /**
     * Clean up event listeners
     */
    destroy() {
      if (cleanup) {
        cleanup();
        cleanup = null;
      }
    },

    /**
     * Check if the server is reachable
     */
    async checkServerReachability(endpoint = '/api/health'): Promise<boolean> {
      try {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 5000);

        const response = await fetch(endpoint, {
          method: 'HEAD',
          signal: controller.signal,
        });

        clearTimeout(timeout);

        const reachable = response.ok;
        update((s) => ({
          ...s,
          serverReachable: reachable,
          lastOnlineAt: reachable ? new Date() : s.lastOnlineAt,
          lastSyncError: reachable ? null : s.lastSyncError,
        }));

        return reachable;
      } catch {
        update((s) => ({ ...s, serverReachable: false }));
        return false;
      }
    },

    /**
     * Set the pending sync count
     */
    setPendingSyncCount(count: number) {
      update((s) => ({ ...s, pendingSyncCount: count }));
    },

    /**
     * Increment pending sync count
     */
    incrementPendingSync() {
      update((s) => ({ ...s, pendingSyncCount: s.pendingSyncCount + 1 }));
    },

    /**
     * Decrement pending sync count
     */
    decrementPendingSync() {
      update((s) => ({
        ...s,
        pendingSyncCount: Math.max(0, s.pendingSyncCount - 1),
      }));
    },

    /**
     * Set syncing state
     */
    setSyncing(isSyncing: boolean) {
      update((s) => ({ ...s, isSyncing }));
    },

    /**
     * Set sync error
     */
    setSyncError(error: string | null) {
      update((s) => ({ ...s, lastSyncError: error }));
    },

    /**
     * Reset the store
     */
    reset() {
      set(defaultState);
    },
  };
}

// Type for Navigator with connection info
interface NavigatorWithConnection extends Navigator {
  connection?: {
    type?: string;
    effectiveType?: string;
    addEventListener: (event: string, callback: () => void) => void;
    removeEventListener: (event: string, callback: () => void) => void;
  };
}

export const network = createNetworkStore();

// Derived stores for common checks
export const isOnline = derived(network, ($network) => $network.online);
export const isOffline = derived(network, ($network) => !$network.online);
export const hasPendingSyncs = derived(network, ($network) => $network.pendingSyncCount > 0);
export const canSync = derived(
  network,
  ($network) => $network.online && $network.serverReachable && !$network.isSyncing
);
