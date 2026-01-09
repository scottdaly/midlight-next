// Window state persistence store
// Saves and restores window position, size, and maximized state

import { writable, derived, get } from 'svelte/store';

// ============================================================================
// Types
// ============================================================================

export interface WindowState {
  width: number;
  height: number;
  x: number | null;
  y: number | null;
  maximized: boolean;
  fullscreen: boolean;
}

interface WindowStateStore {
  state: WindowState | null;
  loaded: boolean;
  error: string | null;
}

// ============================================================================
// Default Values
// ============================================================================

const DEFAULT_WINDOW_STATE: WindowState = {
  width: 1200,
  height: 800,
  x: null, // Center on screen
  y: null,
  maximized: false,
  fullscreen: false,
};

const STORAGE_KEY = 'window-state';

// ============================================================================
// Store Creation
// ============================================================================

function createWindowStateStore() {
  const { subscribe, set, update } = writable<WindowStateStore>({
    state: null,
    loaded: false,
    error: null,
  });

  return {
    subscribe,

    /**
     * Set the loaded window state
     */
    setLoaded(state: WindowState | null) {
      update((s) => ({
        ...s,
        state: state || DEFAULT_WINDOW_STATE,
        loaded: true,
        error: null,
      }));
    },

    /**
     * Update the current window state (call this on window move/resize)
     */
    updateState(partial: Partial<WindowState>) {
      update((s) => ({
        ...s,
        state: s.state ? { ...s.state, ...partial } : { ...DEFAULT_WINDOW_STATE, ...partial },
      }));
    },

    /**
     * Set error state
     */
    setError(error: string) {
      update((s) => ({
        ...s,
        error,
        loaded: true,
      }));
    },

    /**
     * Get the current state for saving
     */
    getState(): WindowState | null {
      return get({ subscribe }).state;
    },

    /**
     * Get the storage key
     */
    getStorageKey(): string {
      return STORAGE_KEY;
    },

    /**
     * Get default state
     */
    getDefaultState(): WindowState {
      return DEFAULT_WINDOW_STATE;
    },
  };
}

// ============================================================================
// Exports
// ============================================================================

export const windowStateStore = createWindowStateStore();

// Derived stores for convenience
export const windowState = derived(windowStateStore, ($store) => $store.state);
export const windowStateLoaded = derived(windowStateStore, ($store) => $store.loaded);
