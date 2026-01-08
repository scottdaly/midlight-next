// File Watcher client - Tauri invoke wrappers for file watching
// Monitors workspace files for external changes (outside the app)

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ============================================================================
// Types (matching Rust types)
// ============================================================================

export interface FileChangeEvent {
  change_type: 'modify' | 'create' | 'delete';
  file_key: string;
  timestamp: string;
}

// Transformed type for frontend use (camelCase)
export interface FileChange {
  changeType: 'modify' | 'create' | 'delete';
  fileKey: string;
  timestamp: Date;
}

// ============================================================================
// File Watcher Client
// ============================================================================

class FileWatcherClient {
  private unlistenFn: UnlistenFn | null = null;
  private changeCallback: ((change: FileChange) => void) | null = null;

  /**
   * Start watching a workspace for file changes
   */
  async start(workspaceRoot: string): Promise<void> {
    await invoke('file_watcher_start', { workspaceRoot });
  }

  /**
   * Stop watching a workspace
   */
  async stop(workspaceRoot: string): Promise<void> {
    await invoke('file_watcher_stop', { workspaceRoot });
  }

  /**
   * Mark a file as being saved by the app (to ignore the change event)
   * Call this BEFORE writing to a file
   */
  async markSaving(workspaceRoot: string, fileKey: string): Promise<void> {
    await invoke('file_watcher_mark_saving', { workspaceRoot, fileKey });
  }

  /**
   * Clear the saving mark after save completes
   * Call this AFTER writing to a file
   */
  async clearSaving(workspaceRoot: string, fileKey: string): Promise<void> {
    await invoke('file_watcher_clear_saving', { workspaceRoot, fileKey });
  }

  /**
   * Listen for file change events from the backend
   * Returns an unlisten function to stop listening
   */
  async onFileChange(callback: (change: FileChange) => void): Promise<UnlistenFn> {
    // Store callback for internal use
    this.changeCallback = callback;

    // Listen for events from Rust
    this.unlistenFn = await listen<FileChangeEvent>('file-watcher:change', (event) => {
      const payload = event.payload;
      callback({
        changeType: payload.change_type as FileChange['changeType'],
        fileKey: payload.file_key,
        timestamp: new Date(payload.timestamp),
      });
    });

    return this.unlistenFn;
  }

  /**
   * Stop listening for file change events
   */
  unlisten(): void {
    if (this.unlistenFn) {
      this.unlistenFn();
      this.unlistenFn = null;
      this.changeCallback = null;
    }
  }
}

export const fileWatcherClient = new FileWatcherClient();
