// Updates client - Tauri invoke wrappers for auto-updates

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  updateStore,
  type UpdateInfo,
  type UpdateProgress,
} from '@midlight/stores';

// ============================================================================
// Types (matching Rust types)
// ============================================================================

interface UpdateInfoResponse {
  version: string;
  current_version: string;
  body: string | null;
  date: string | null;
}

interface UpdateProgressEvent {
  downloaded: number;
  total: number | null;
}

// ============================================================================
// Updates Client
// ============================================================================

class UpdatesClient {
  private progressUnlisten: UnlistenFn | null = null;
  private readyUnlisten: UnlistenFn | null = null;
  private checkInterval: ReturnType<typeof setInterval> | null = null;

  // Check interval: 4 hours
  private static CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000;

  // Initial check delay: 10 seconds after app starts
  private static INITIAL_DELAY_MS = 10 * 1000;

  /**
   * Initialize the updates client
   * Sets up event listeners and schedules periodic update checks
   */
  async init(): Promise<void> {
    // Listen for download progress events from Rust
    this.progressUnlisten = await listen<UpdateProgressEvent>(
      'update-download-progress',
      (event) => {
        const progress: UpdateProgress = {
          downloaded: event.payload.downloaded,
          total: event.payload.total ?? undefined,
        };
        updateStore.setProgress(progress);
      }
    );

    // Listen for ready-to-install event
    this.readyUnlisten = await listen('update-ready-to-install', () => {
      updateStore.setReady();
    });

    // Check for updates after initial delay
    setTimeout(() => {
      this.checkForUpdates(false); // Don't show dialog on initial check
    }, UpdatesClient.INITIAL_DELAY_MS);

    // Schedule periodic checks
    this.checkInterval = setInterval(() => {
      this.checkForUpdates(false);
    }, UpdatesClient.CHECK_INTERVAL_MS);
  }

  /**
   * Cleanup event listeners and intervals
   */
  destroy(): void {
    if (this.progressUnlisten) {
      this.progressUnlisten();
      this.progressUnlisten = null;
    }
    if (this.readyUnlisten) {
      this.readyUnlisten();
      this.readyUnlisten = null;
    }
    if (this.checkInterval) {
      clearInterval(this.checkInterval);
      this.checkInterval = null;
    }
  }

  /**
   * Check if an update is available
   * @param showDialog Whether to show the update dialog if available
   */
  async checkForUpdates(showDialog = true): Promise<void> {
    updateStore.startCheck();

    try {
      const response = await invoke<UpdateInfoResponse | null>(
        'check_for_updates'
      );

      if (response) {
        const info: UpdateInfo = {
          version: response.version,
          currentVersion: response.current_version,
          body: response.body ?? undefined,
          date: response.date ?? undefined,
        };
        updateStore.setUpdateAvailable(info, showDialog);
      } else {
        updateStore.setNoUpdate();
      }
    } catch (error) {
      const message =
        error instanceof Error ? error.message : 'Failed to check for updates';
      updateStore.setError(message);
      console.error('Update check failed:', error);
    }
  }

  /**
   * Download and install the available update
   * The update will be applied on next app restart
   */
  async downloadAndInstall(): Promise<void> {
    updateStore.startDownload();

    try {
      await invoke('download_and_install_update');
      // Success - the ready event will be emitted
    } catch (error) {
      const message =
        error instanceof Error ? error.message : 'Failed to download update';
      updateStore.setError(message);
      console.error('Update download failed:', error);
    }
  }

  /**
   * Get the current app version
   */
  async getCurrentVersion(): Promise<string> {
    return invoke<string>('get_current_version');
  }
}

// Export singleton instance
export const updatesClient = new UpdatesClient();
