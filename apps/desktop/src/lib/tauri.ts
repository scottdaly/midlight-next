// Tauri Storage Adapter - Implements StorageAdapter using Tauri commands

import { invoke } from '@tauri-apps/api/core';
import type {
  StorageAdapter,
  FileNode,
  Checkpoint,
  TiptapDocument,
  LoadedDocument,
  SaveResult,
  CheckpointTrigger,
} from '@midlight/core/types';

export class TauriStorageAdapter implements StorageAdapter {
  // Lifecycle
  async init(): Promise<void> {
    // No initialization needed for Tauri adapter
    // Tauri commands are ready immediately
  }

  // File operations
  async readDir(path: string): Promise<FileNode[]> {
    return await invoke('read_dir', { path });
  }

  async readFile(path: string): Promise<string> {
    return await invoke('read_file', { path });
  }

  async writeFile(path: string, content: string): Promise<void> {
    await invoke('write_file', { path, content });
  }

  async deleteFile(path: string): Promise<void> {
    await invoke('delete_file', { path });
  }

  async renameFile(oldPath: string, newPath: string): Promise<void> {
    await invoke('rename_file', { oldPath, newPath });
  }

  async fileExists(path: string): Promise<boolean> {
    return await invoke('file_exists', { path });
  }

  async createFile(parentPath: string, name: string): Promise<FileNode> {
    return await invoke('create_midlight_file', { parentPath, name });
  }

  async createFolder(parentPath: string, name: string): Promise<FileNode> {
    return await invoke('create_new_folder', { parentPath, name });
  }

  // Document operations (with sidecar handling)
  async loadDocument(workspaceRoot: string, filePath: string): Promise<LoadedDocument> {
    return await invoke('workspace_load_document', {
      workspaceRoot,
      filePath,
    });
  }

  async saveDocument(
    workspaceRoot: string,
    filePath: string,
    json: TiptapDocument,
    trigger: CheckpointTrigger
  ): Promise<SaveResult> {
    return await invoke('workspace_save_document', {
      workspaceRoot,
      filePath,
      json,
      trigger,
    });
  }

  // Workspace operations
  async initWorkspace(path: string): Promise<void> {
    await invoke('workspace_init', { workspaceRoot: path });
  }

  async getCheckpoints(workspaceRoot: string, filePath: string): Promise<Checkpoint[]> {
    return await invoke('get_checkpoints', {
      workspaceRoot,
      filePath,
    });
  }

  async restoreCheckpoint(
    workspaceRoot: string,
    filePath: string,
    checkpointId: string
  ): Promise<TiptapDocument> {
    return await invoke('restore_checkpoint', {
      workspaceRoot,
      filePath,
      checkpointId,
    });
  }
}
