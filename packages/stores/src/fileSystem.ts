// @midlight/stores/fileSystem - File system state management

import { writable, derived, get } from 'svelte/store';
import type {
  FileNode,
  TiptapDocument,
  StorageAdapter,
  CheckpointTrigger,
} from '@midlight/core/types';
import { createMergedDiffDocument } from './utils/diff.js';

export interface PendingDiff {
  changeId: string;
  path: string;
  content: string;
  originalContent: string;
}

export interface PendingNewItem {
  type: 'file' | 'folder';
  parentPath: string;
  defaultName: string;
}

export interface StagedEdit {
  changeId: string;
  path: string;
  originalTiptapJson: TiptapDocument;
  stagedTiptapJson: TiptapDocument;
  originalText: string;
  newText: string;
  description?: string;
  createdAt: string;
}

export interface FileSystemState {
  rootDir: string | null;
  files: FileNode[];
  openFiles: FileNode[];
  activeFilePath: string | null;
  editorContent: TiptapDocument | null;
  contentRevision: number; // Incremented to force editor refresh
  isDirty: boolean;
  isSaving: boolean;
  lastSavedAt: Date | null;
  autoSaveEnabled: boolean;
  autoSaveInterval: number; // ms, default 3000
  pendingDiffs: Record<string, PendingDiff>;
  hasRecovery: boolean;
  recoveryTime?: string;
  // Multi-selection state
  selectedPaths: string[];
  lastSelectedPath: string | null;
  // Clipboard state
  clipboardPaths: string[];
  clipboardOperation: 'copy' | 'cut' | null;
  // Pending new item (for inline rename UI)
  pendingNewItem: PendingNewItem | null;
  // Staged edit (for visual diff accept/reject)
  stagedEdit: StagedEdit | null;
}

const initialState: FileSystemState = {
  rootDir: null,
  files: [],
  openFiles: [],
  activeFilePath: null,
  editorContent: null,
  contentRevision: 0,
  isDirty: false,
  isSaving: false,
  lastSavedAt: null,
  autoSaveEnabled: true,
  autoSaveInterval: 3000,
  pendingDiffs: {},
  hasRecovery: false,
  // Multi-selection
  selectedPaths: [],
  lastSelectedPath: null,
  // Clipboard
  clipboardPaths: [],
  clipboardOperation: null,
  // Pending new item
  pendingNewItem: null,
  // Staged edit
  stagedEdit: null,
};

function createFileSystemStore() {
  const { subscribe, set, update } = writable<FileSystemState>(initialState);

  // Storage adapter will be set based on platform (Tauri or Web)
  let storageAdapter: StorageAdapter | null = null;

  return {
    subscribe,

    /**
     * Sets the storage adapter (Tauri or Web)
     */
    setStorageAdapter(adapter: StorageAdapter) {
      storageAdapter = adapter;
    },

    /**
     * Sets the workspace root directory
     */
    setRootDir(path: string) {
      update((s) => ({ ...s, rootDir: path }));
    },

    /**
     * Loads the file tree from the storage adapter
     */
    async loadDir(path: string) {
      if (!storageAdapter) throw new Error('Storage adapter not set');

      const files = await storageAdapter.readDir(path);
      update((s) => ({
        ...s,
        rootDir: path,
        files,
      }));
    },

    /**
     * Refreshes the file tree
     */
    async refresh() {
      const state = get({ subscribe });
      if (state.rootDir && storageAdapter) {
        const files = await storageAdapter.readDir(state.rootDir);
        update((s) => ({ ...s, files }));
      }
    },

    /**
     * Reloads the content of a specific file if it's currently open
     */
    async reloadDocument(filePath: string) {
      if (!storageAdapter) return;
      const state = get({ subscribe });
      if (!state.rootDir) return;

      // Check if this file is currently open
      const isOpen = state.openFiles.some((f) => f.path === filePath);
      if (!isOpen) return;

      // Reload the document content
      try {
        const result = await storageAdapter.loadDocument(state.rootDir, filePath);

        // Only update if this is the active file
        if (state.activeFilePath === filePath) {
          update((s) => ({
            ...s,
            editorContent: result.json,
            contentRevision: s.contentRevision + 1, // Force editor to refresh
            isDirty: false,
          }));
        }
      } catch (error) {
        console.error('Failed to reload document:', error);
      }
    },

    /**
     * Opens a file
     */
    async openFile(file: FileNode) {
      if (!storageAdapter) throw new Error('Storage adapter not set');
      const state = get({ subscribe });
      if (!state.rootDir) throw new Error('No workspace open');

      const result = await storageAdapter.loadDocument(state.rootDir, file.path);

      update((s) => ({
        ...s,
        openFiles: s.openFiles.find((f) => f.path === file.path)
          ? s.openFiles
          : [...s.openFiles, file],
        activeFilePath: file.path,
        editorContent: result.json,
        isDirty: false,
        hasRecovery: result.hasRecovery,
        recoveryTime: result.recoveryTime,
      }));
    },

    /**
     * Closes a file
     */
    closeFile(filePath: string) {
      update((s) => {
        const newOpenFiles = s.openFiles.filter((f) => f.path !== filePath);
        const newActivePath =
          s.activeFilePath === filePath
            ? newOpenFiles[newOpenFiles.length - 1]?.path || null
            : s.activeFilePath;

        return {
          ...s,
          openFiles: newOpenFiles,
          activeFilePath: newActivePath,
          editorContent: newActivePath === s.activeFilePath ? s.editorContent : null,
          isDirty: false,
        };
      });
    },

    /**
     * Sets the active file by path (and loads its content)
     */
    async setActiveFile(filePath: string) {
      if (!storageAdapter) throw new Error('Storage adapter not set');
      const state = get({ subscribe });
      if (!state.rootDir) throw new Error('No workspace open');

      // Don't reload if already active
      if (state.activeFilePath === filePath) return;

      // Load the document content
      const result = await storageAdapter.loadDocument(state.rootDir, filePath);

      update((s) => ({
        ...s,
        activeFilePath: filePath,
        editorContent: result.json,
        isDirty: false,
        hasRecovery: result.hasRecovery,
        recoveryTime: result.recoveryTime,
      }));
    },

    /**
     * Sets the active tab by index (and loads its content)
     */
    async setActiveTab(index: number) {
      const state = get({ subscribe });
      const file = state.openFiles[index];
      if (!file) return;
      await this.setActiveFile(file.path);
    },

    /**
     * Reorders tabs (for drag-and-drop)
     */
    reorderTabs(fromIndex: number, toIndex: number) {
      update((s) => {
        const newOpenFiles = [...s.openFiles];
        const [moved] = newOpenFiles.splice(fromIndex, 1);
        if (moved) {
          newOpenFiles.splice(toIndex, 0, moved);
        }
        return { ...s, openFiles: newOpenFiles };
      });
    },

    /**
     * Updates the editor content
     */
    setEditorContent(content: TiptapDocument | null) {
      update((s) => ({ ...s, editorContent: content }));
    },

    /**
     * Marks the editor as dirty
     */
    setIsDirty(isDirty: boolean) {
      update((s) => ({ ...s, isDirty }));
    },

    /**
     * Saves the current document
     */
    async save(trigger: CheckpointTrigger = 'interval') {
      if (!storageAdapter) throw new Error('Storage adapter not set');
      const state = get({ subscribe });

      if (!state.rootDir || !state.activeFilePath || !state.editorContent) {
        return;
      }

      // Set saving state
      update((s) => ({ ...s, isSaving: true }));

      try {
        await storageAdapter.saveDocument(
          state.rootDir,
          state.activeFilePath,
          state.editorContent,
          trigger
        );

        update((s) => ({ ...s, isDirty: false, isSaving: false, lastSavedAt: new Date() }));
      } catch (error) {
        update((s) => ({ ...s, isSaving: false }));
        throw error;
      }
    },

    /**
     * Sets auto-save settings
     */
    setAutoSave(enabled: boolean, interval?: number) {
      update((s) => ({
        ...s,
        autoSaveEnabled: enabled,
        autoSaveInterval: interval ?? s.autoSaveInterval,
      }));
    },

    /**
     * Adds a pending diff (from AI agent)
     */
    addPendingDiff(diff: PendingDiff) {
      update((s) => ({
        ...s,
        pendingDiffs: { ...s.pendingDiffs, [diff.changeId]: diff },
      }));
    },

    /**
     * Removes a pending diff
     */
    removePendingDiff(changeId: string) {
      update((s) => {
        const { [changeId]: _, ...rest } = s.pendingDiffs;
        return { ...s, pendingDiffs: rest };
      });
    },

    /**
     * Clears all pending diffs
     */
    clearPendingDiffs() {
      update((s) => ({ ...s, pendingDiffs: {} }));
    },

    // ============== MULTI-SELECTION ==============

    /**
     * Selects a file with the given mode
     * @param path - The file path to select
     * @param mode - 'single' replaces selection, 'toggle' adds/removes, 'range' selects range
     * @param allPaths - Required for 'range' mode - flattened list of all visible paths in order
     */
    selectFile(path: string, mode: 'single' | 'toggle' | 'range', allPaths?: string[]) {
      update((s) => {
        if (mode === 'single') {
          return {
            ...s,
            selectedPaths: [path],
            lastSelectedPath: path,
          };
        }

        if (mode === 'toggle') {
          const isSelected = s.selectedPaths.includes(path);
          return {
            ...s,
            selectedPaths: isSelected
              ? s.selectedPaths.filter((p) => p !== path)
              : [...s.selectedPaths, path],
            lastSelectedPath: path,
          };
        }

        if (mode === 'range' && allPaths && s.lastSelectedPath) {
          const lastIndex = allPaths.indexOf(s.lastSelectedPath);
          const currentIndex = allPaths.indexOf(path);

          if (lastIndex === -1 || currentIndex === -1) {
            return { ...s, selectedPaths: [path], lastSelectedPath: path };
          }

          const start = Math.min(lastIndex, currentIndex);
          const end = Math.max(lastIndex, currentIndex);
          const rangePaths = allPaths.slice(start, end + 1);

          // Merge with existing selection (keep non-range items)
          const existingNonRange = s.selectedPaths.filter((p) => !allPaths.includes(p));
          return {
            ...s,
            selectedPaths: [...new Set([...existingNonRange, ...rangePaths])],
            // Don't update lastSelectedPath for range selection
          };
        }

        // Fallback to single selection
        return { ...s, selectedPaths: [path], lastSelectedPath: path };
      });
    },

    /**
     * Clears all selection
     */
    clearSelection() {
      update((s) => ({
        ...s,
        selectedPaths: [],
        lastSelectedPath: null,
      }));
    },

    /**
     * Checks if a path is selected
     */
    isSelected(path: string): boolean {
      return get({ subscribe }).selectedPaths.includes(path);
    },

    // ============== CLIPBOARD ==============

    /**
     * Copies paths to clipboard (internal app clipboard)
     */
    copyToClipboard(paths: string[]) {
      update((s) => ({
        ...s,
        clipboardPaths: paths,
        clipboardOperation: 'copy',
      }));
    },

    /**
     * Cuts paths to clipboard (internal app clipboard)
     */
    cutToClipboard(paths: string[]) {
      update((s) => ({
        ...s,
        clipboardPaths: paths,
        clipboardOperation: 'cut',
      }));
    },

    /**
     * Clears the clipboard
     */
    clearClipboard() {
      update((s) => ({
        ...s,
        clipboardPaths: [],
        clipboardOperation: null,
      }));
    },

    /**
     * Gets clipboard state
     */
    getClipboard() {
      const state = get({ subscribe });
      return {
        paths: state.clipboardPaths,
        operation: state.clipboardOperation,
      };
    },

    // ============== FILE/FOLDER CREATION ==============

    /**
     * Starts the inline rename flow for creating a new file
     * @param parentPath - Parent directory to create in (defaults to rootDir)
     */
    startNewFile(parentPath?: string) {
      const state = get({ subscribe });
      const targetPath = parentPath || state.rootDir;
      if (!targetPath) return;

      update((s) => ({
        ...s,
        pendingNewItem: {
          type: 'file',
          parentPath: targetPath,
          defaultName: 'Untitled',
        },
      }));
    },

    /**
     * Starts the inline rename flow for creating a new folder
     * @param parentPath - Parent directory to create in (defaults to rootDir)
     */
    startNewFolder(parentPath?: string) {
      const state = get({ subscribe });
      const targetPath = parentPath || state.rootDir;
      if (!targetPath) return;

      update((s) => ({
        ...s,
        pendingNewItem: {
          type: 'folder',
          parentPath: targetPath,
          defaultName: 'New Folder',
        },
      }));
    },

    /**
     * Confirms the pending new item creation with the given name
     * @param name - The name to use for the new item
     */
    async confirmNewItem(name: string): Promise<FileNode | null> {
      const state = get({ subscribe });
      if (!state.pendingNewItem) return null;

      const { type, parentPath } = state.pendingNewItem;

      // Clear pending state first
      update((s) => ({ ...s, pendingNewItem: null }));

      if (type === 'file') {
        return await this.createFile(parentPath, name, true);
      } else {
        return await this.createFolder(parentPath, name);
      }
    },

    /**
     * Cancels the pending new item creation
     */
    cancelNewItem() {
      update((s) => ({ ...s, pendingNewItem: null }));
    },

    /**
     * Creates a new .midlight file
     * @param parentPath - Parent directory path
     * @param name - File name (without extension, will be added)
     * @param openAfterCreate - Whether to open the file after creation
     * @returns The created FileNode
     */
    async createFile(
      parentPath: string,
      name: string = 'Untitled',
      openAfterCreate: boolean = true
    ): Promise<FileNode | null> {
      if (!storageAdapter) throw new Error('Storage adapter not set');
      const state = get({ subscribe });
      if (!state.rootDir) throw new Error('No workspace open');

      try {
        const newFile = await storageAdapter.createFile(parentPath, name);

        // Refresh file tree
        await this.refresh();

        // Open the new file if requested
        if (openAfterCreate && newFile) {
          await this.openFile(newFile);
        }

        return newFile;
      } catch (error) {
        console.error('Failed to create file:', error);
        throw error;
      }
    },

    /**
     * Creates a new folder
     * @param parentPath - Parent directory path
     * @param name - Folder name
     * @returns The created FileNode
     */
    async createFolder(parentPath: string, name: string = 'New Folder'): Promise<FileNode | null> {
      if (!storageAdapter) throw new Error('Storage adapter not set');
      const state = get({ subscribe });
      if (!state.rootDir) throw new Error('No workspace open');

      try {
        const newFolder = await storageAdapter.createFolder(parentPath, name);

        // Refresh file tree
        await this.refresh();

        return newFolder;
      } catch (error) {
        console.error('Failed to create folder:', error);
        throw error;
      }
    },

    // ============== STAGED EDITS (Visual Diff) ==============

    /**
     * Stages an edit for visual diff display
     * @param edit - The staged edit data from the agent
     */
    stageEdit(edit: StagedEdit) {
      // Create a merged diff document that shows both removed and added content
      // with appropriate diff marks for visual display
      const mergedDiffDoc = createMergedDiffDocument(
        edit.originalTiptapJson as Parameters<typeof createMergedDiffDocument>[0],
        edit.stagedTiptapJson as Parameters<typeof createMergedDiffDocument>[0]
      );

      update((s) => ({
        ...s,
        stagedEdit: edit,
        // Update editor content to show the merged diff view
        editorContent: mergedDiffDoc as TiptapDocument,
        contentRevision: s.contentRevision + 1,
      }));
    },

    /**
     * Accepts the staged edit - writes to disk and clears staged state
     */
    async acceptStagedEdit() {
      const state = get({ subscribe });
      const staged = state.stagedEdit;
      if (!staged || !storageAdapter || !state.rootDir) return;

      try {
        // Write the staged content to disk
        await storageAdapter.saveDocument(
          state.rootDir,
          staged.path,
          staged.stagedTiptapJson,
          'manual'
        );

        // Clear staged state and keep the new content
        update((s) => ({
          ...s,
          stagedEdit: null,
          editorContent: staged.stagedTiptapJson,
          isDirty: false,
          lastSavedAt: new Date(),
        }));

        // Refresh file tree
        await this.refresh();
      } catch (error) {
        console.error('Failed to accept staged edit:', error);
        throw error;
      }
    },

    /**
     * Rejects the staged edit - restores original content, no disk write
     */
    rejectStagedEdit() {
      const state = get({ subscribe });
      const staged = state.stagedEdit;
      if (!staged) return;

      // Restore original content in editor
      update((s) => ({
        ...s,
        stagedEdit: null,
        editorContent: staged.originalTiptapJson,
        contentRevision: s.contentRevision + 1,
      }));
    },

    /**
     * Checks if there's a staged edit for a specific path
     */
    hasStagedEditFor(path: string): boolean {
      const state = get({ subscribe });
      return state.stagedEdit?.path === path;
    },

    /**
     * Resets the store
     */
    reset() {
      set(initialState);
    },
  };
}

export const fileSystem = createFileSystemStore();

// Derived stores
export const activeFile = derived(fileSystem, ($fs) =>
  $fs.openFiles.find((f) => f.path === $fs.activeFilePath)
);

export const activeFileIndex = derived(fileSystem, ($fs) =>
  $fs.openFiles.findIndex((f) => f.path === $fs.activeFilePath)
);

export const isSaving = derived(fileSystem, ($fs) => $fs.isSaving);

export const hasPendingDiffs = derived(
  fileSystem,
  ($fs) => Object.keys($fs.pendingDiffs).length > 0
);

export const selectedPaths = derived(fileSystem, ($fs) => $fs.selectedPaths);

export const selectionCount = derived(fileSystem, ($fs) => $fs.selectedPaths.length);

export const hasClipboard = derived(
  fileSystem,
  ($fs) => $fs.clipboardPaths.length > 0
);

export const clipboardOperation = derived(fileSystem, ($fs) => $fs.clipboardOperation);

export const pendingNewItem = derived(fileSystem, ($fs) => $fs.pendingNewItem);

export const stagedEdit = derived(fileSystem, ($fs) => $fs.stagedEdit);

export const hasStagedEdit = derived(fileSystem, ($fs) => $fs.stagedEdit !== null);
