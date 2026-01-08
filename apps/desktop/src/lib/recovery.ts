// Recovery client for Tauri backend
// Handles crash recovery operations via IPC

import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// Types (matching Rust types)
// ============================================================================

export interface RecoveryFile {
  file_key: string;
  wal_content: string;
  wal_time: string;
  workspace_root: string;
}

// Transformed type for frontend use (camelCase)
export interface RecoveryFileInfo {
  fileKey: string;
  walContent: string;
  walTime: Date;
  workspaceRoot: string;
}

// ============================================================================
// Recovery Client
// ============================================================================

class RecoveryClient {
  /**
   * Check for recovery files on startup
   * Returns list of files with unsaved changes
   */
  async checkForRecovery(workspaceRoot: string): Promise<RecoveryFileInfo[]> {
    const files = await invoke<RecoveryFile[]>('recovery_check', {
      workspaceRoot,
    });

    return files.map((f) => ({
      fileKey: f.file_key,
      walContent: f.wal_content,
      walTime: new Date(f.wal_time),
      workspaceRoot: f.workspace_root,
    }));
  }

  /**
   * Write WAL file for a document
   * Returns true if content was written (changed), false if skipped (unchanged)
   */
  async writeWal(workspaceRoot: string, fileKey: string, content: string): Promise<boolean> {
    return invoke<boolean>('recovery_write_wal', {
      workspaceRoot,
      fileKey,
      content,
    });
  }

  /**
   * Clear WAL file after successful save
   */
  async clearWal(workspaceRoot: string, fileKey: string): Promise<void> {
    await invoke('recovery_clear_wal', {
      workspaceRoot,
      fileKey,
    });
  }

  /**
   * Check if a specific file has recovery available
   */
  async hasRecovery(workspaceRoot: string, fileKey: string): Promise<boolean> {
    return invoke<boolean>('recovery_has_recovery', {
      workspaceRoot,
      fileKey,
    });
  }

  /**
   * Get recovery content for a specific file
   */
  async getRecoveryContent(workspaceRoot: string, fileKey: string): Promise<string | null> {
    return invoke<string | null>('recovery_get_content', {
      workspaceRoot,
      fileKey,
    });
  }

  /**
   * Discard recovery for a specific file (user chose not to recover)
   */
  async discardRecovery(workspaceRoot: string, fileKey: string): Promise<void> {
    await invoke('recovery_discard', {
      workspaceRoot,
      fileKey,
    });
  }

  /**
   * Discard all recovery files for a workspace
   */
  async discardAllRecovery(workspaceRoot: string): Promise<void> {
    await invoke('recovery_discard_all', {
      workspaceRoot,
    });
  }

  /**
   * Check if recovery content differs from current file content
   */
  async hasUniqueContent(
    workspaceRoot: string,
    fileKey: string,
    currentContent: string
  ): Promise<boolean> {
    return invoke<boolean>('recovery_has_unique_content', {
      workspaceRoot,
      fileKey,
      currentContent,
    });
  }
}

export const recoveryClient = new RecoveryClient();
