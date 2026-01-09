// Window state client - Persist window position and size using Tauri store
// Uses tauri-plugin-store for persistence

import {
  getCurrentWindow,
  LogicalSize,
  LogicalPosition,
  type Window,
} from '@tauri-apps/api/window';
import { LazyStore } from '@tauri-apps/plugin-store';
import { windowStateStore, type WindowState } from '@midlight/stores';

const STORE_NAME = 'window-state.json';
const STATE_KEY = 'state';

// Debounce timer for saving state
let saveTimeout: ReturnType<typeof setTimeout> | null = null;
const SAVE_DEBOUNCE_MS = 500;

class WindowStateClient {
  private store: LazyStore | null = null;
  private window: Window | null = null;
  private initialized = false;

  /**
   * Initialize the window state client
   * Loads saved state and sets up event listeners
   */
  async init(): Promise<void> {
    if (this.initialized) return;

    try {
      // Get the main window
      this.window = getCurrentWindow();

      // Load the store (LazyStore auto-saves)
      this.store = new LazyStore(STORE_NAME);

      // Load saved state
      const savedState = await this.store.get<WindowState>(STATE_KEY);

      if (savedState) {
        windowStateStore.setLoaded(savedState);

        // Apply saved state to window
        await this.applyState(savedState);
      } else {
        // No saved state, use defaults
        windowStateStore.setLoaded(null);
      }

      // Set up event listeners for window changes
      this.setupListeners();

      this.initialized = true;
    } catch (error) {
      console.error('Failed to initialize window state:', error);
      windowStateStore.setError(
        error instanceof Error ? error.message : 'Failed to load window state'
      );
    }
  }

  /**
   * Apply saved state to the window
   */
  private async applyState(state: WindowState): Promise<void> {
    if (!this.window) return;

    try {
      // Set size first
      await this.window.setSize(new LogicalSize(state.width, state.height));

      // Set position if we have it
      if (state.x !== null && state.y !== null) {
        await this.window.setPosition(new LogicalPosition(state.x, state.y));
      }

      // Set maximized state
      if (state.maximized) {
        await this.window.maximize();
      }

      // Set fullscreen state
      if (state.fullscreen) {
        await this.window.setFullscreen(true);
      }
    } catch (error) {
      console.error('Failed to apply window state:', error);
    }
  }

  /**
   * Set up listeners for window events
   */
  private setupListeners(): void {
    if (!this.window) return;

    // Listen for window move
    this.window.onMoved(async () => {
      await this.updateAndSave();
    });

    // Listen for window resize
    this.window.onResized(async () => {
      await this.updateAndSave();
    });

    // Listen for close request - save immediately
    this.window.onCloseRequested(async () => {
      // Cancel any pending debounced save
      if (saveTimeout) {
        clearTimeout(saveTimeout);
        saveTimeout = null;
      }
      // Save immediately
      await this.saveState();
    });
  }

  /**
   * Update store state and schedule a debounced save
   */
  private async updateAndSave(): Promise<void> {
    if (!this.window) return;

    try {
      // Get current window state
      const [size, position, isMaximized, isFullscreen] = await Promise.all([
        this.window.innerSize(),
        this.window.outerPosition(),
        this.window.isMaximized(),
        this.window.isFullscreen(),
      ]);

      // Don't save position if maximized or fullscreen (it's not meaningful)
      const state: WindowState = {
        width: size.width,
        height: size.height,
        x: isMaximized || isFullscreen ? null : position.x,
        y: isMaximized || isFullscreen ? null : position.y,
        maximized: isMaximized,
        fullscreen: isFullscreen,
      };

      // Update store
      windowStateStore.updateState(state);

      // Debounce the save
      if (saveTimeout) {
        clearTimeout(saveTimeout);
      }
      saveTimeout = setTimeout(() => {
        this.saveState();
        saveTimeout = null;
      }, SAVE_DEBOUNCE_MS);
    } catch (error) {
      console.error('Failed to update window state:', error);
    }
  }

  /**
   * Save current state to disk
   */
  private async saveState(): Promise<void> {
    if (!this.store) return;

    const state = windowStateStore.getState();
    if (!state) return;

    try {
      await this.store.set(STATE_KEY, state);
      await this.store.save();
    } catch (error) {
      console.error('Failed to save window state:', error);
    }
  }

  /**
   * Clean up resources
   */
  destroy(): void {
    if (saveTimeout) {
      clearTimeout(saveTimeout);
      saveTimeout = null;
    }
  }
}

// Export singleton instance
export const windowStateClient = new WindowStateClient();
