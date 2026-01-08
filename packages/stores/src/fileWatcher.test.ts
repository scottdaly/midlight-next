import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
  fileWatcherStore,
  hasPendingExternalChanges,
  pendingChangeCount,
  showExternalChangeDialog,
  pendingExternalChanges,
  selectedExternalChange,
  isFileWatching,
  changesByType,
  type ExternalChange,
} from './fileWatcher';

describe('fileWatcherStore', () => {
  beforeEach(() => {
    fileWatcherStore.reset();
  });

  describe('initial state', () => {
    it('should have correct initial state', () => {
      const state = get(fileWatcherStore);

      expect(state.isWatching).toBe(false);
      expect(state.workspaceRoot).toBeNull();
      expect(state.pendingChanges).toHaveLength(0);
      expect(state.showDialog).toBe(false);
      expect(state.selectedChangeIndex).toBe(0);
      expect(state.error).toBeNull();
    });
  });

  describe('startWatching/stopWatching', () => {
    it('should set watching state', () => {
      fileWatcherStore.startWatching('/path/to/workspace');

      const state = get(fileWatcherStore);
      expect(state.isWatching).toBe(true);
      expect(state.workspaceRoot).toBe('/path/to/workspace');
      expect(state.error).toBeNull();
    });

    it('should clear error on start', () => {
      fileWatcherStore.setError('Previous error');
      fileWatcherStore.startWatching('/path/to/workspace');

      const state = get(fileWatcherStore);
      expect(state.error).toBeNull();
    });

    it('should clear watching state on stop', () => {
      fileWatcherStore.startWatching('/path/to/workspace');
      fileWatcherStore.stopWatching();

      const state = get(fileWatcherStore);
      expect(state.isWatching).toBe(false);
      expect(state.workspaceRoot).toBeNull();
    });

    it('should keep pending changes on stop', () => {
      fileWatcherStore.startWatching('/path/to/workspace');
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.stopWatching();

      const state = get(fileWatcherStore);
      expect(state.pendingChanges).toHaveLength(1);
    });
  });

  describe('addChange', () => {
    it('should add a new change', () => {
      const change: ExternalChange = {
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      };

      fileWatcherStore.addChange(change);

      const state = get(fileWatcherStore);
      expect(state.pendingChanges).toHaveLength(1);
      expect(state.pendingChanges[0].fileKey).toBe('test.md');
      expect(state.pendingChanges[0].changeType).toBe('modify');
    });

    it('should show dialog when changes exist', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      const state = get(fileWatcherStore);
      expect(state.showDialog).toBe(true);
    });

    it('should deduplicate by fileKey', () => {
      const timestamp1 = new Date('2024-01-01');
      const timestamp2 = new Date('2024-01-02');

      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: timestamp1,
      });

      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: timestamp2,
      });

      const state = get(fileWatcherStore);
      expect(state.pendingChanges).toHaveLength(1);
      expect(state.pendingChanges[0].timestamp).toBe(timestamp2);
    });

    it('should escalate modify -> delete', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'delete',
        timestamp: new Date(),
      });

      const state = get(fileWatcherStore);
      expect(state.pendingChanges[0].changeType).toBe('delete');
    });

    it('should not downgrade delete -> modify', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'delete',
        timestamp: new Date(),
      });

      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      const state = get(fileWatcherStore);
      expect(state.pendingChanges[0].changeType).toBe('delete');
    });

    it('should reset decision on new change', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      fileWatcherStore.setDecision('test.md', 'reload');

      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      const state = get(fileWatcherStore);
      expect(state.pendingChanges[0].decision).toBeUndefined();
    });
  });

  describe('removeChange', () => {
    beforeEach(() => {
      fileWatcherStore.addChange({
        fileKey: 'file1.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'file2.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
    });

    it('should remove a change by fileKey', () => {
      fileWatcherStore.removeChange('file1.md');

      const state = get(fileWatcherStore);
      expect(state.pendingChanges).toHaveLength(1);
      expect(state.pendingChanges[0].fileKey).toBe('file2.md');
    });

    it('should adjust selectedChangeIndex when needed', () => {
      fileWatcherStore.selectChange(1);
      fileWatcherStore.removeChange('file2.md');

      const state = get(fileWatcherStore);
      expect(state.selectedChangeIndex).toBe(0);
    });

    it('should hide dialog when all changes removed', () => {
      fileWatcherStore.removeChange('file1.md');
      fileWatcherStore.removeChange('file2.md');

      const state = get(fileWatcherStore);
      expect(state.showDialog).toBe(false);
    });
  });

  describe('setDecision', () => {
    beforeEach(() => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
    });

    it('should set decision for a change', () => {
      fileWatcherStore.setDecision('test.md', 'reload');

      const state = get(fileWatcherStore);
      expect(state.pendingChanges[0].decision).toBe('reload');
    });

    it('should update decision', () => {
      fileWatcherStore.setDecision('test.md', 'reload');
      fileWatcherStore.setDecision('test.md', 'keep');

      const state = get(fileWatcherStore);
      expect(state.pendingChanges[0].decision).toBe('keep');
    });
  });

  describe('clearAllChanges', () => {
    beforeEach(() => {
      fileWatcherStore.addChange({
        fileKey: 'file1.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'file2.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.selectChange(1);
    });

    it('should clear all changes', () => {
      fileWatcherStore.clearAllChanges();

      const state = get(fileWatcherStore);
      expect(state.pendingChanges).toHaveLength(0);
    });

    it('should hide dialog', () => {
      fileWatcherStore.clearAllChanges();

      const state = get(fileWatcherStore);
      expect(state.showDialog).toBe(false);
    });

    it('should reset selectedChangeIndex', () => {
      fileWatcherStore.clearAllChanges();

      const state = get(fileWatcherStore);
      expect(state.selectedChangeIndex).toBe(0);
    });
  });

  describe('dialog visibility', () => {
    it('should close dialog', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      fileWatcherStore.closeDialog();

      const state = get(fileWatcherStore);
      expect(state.showDialog).toBe(false);
      expect(state.pendingChanges).toHaveLength(1);
    });

    it('should open dialog when changes exist', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.closeDialog();

      fileWatcherStore.openDialog();

      const state = get(fileWatcherStore);
      expect(state.showDialog).toBe(true);
    });

    it('should not open dialog when no changes', () => {
      fileWatcherStore.openDialog();

      const state = get(fileWatcherStore);
      expect(state.showDialog).toBe(false);
    });
  });

  describe('selectChange', () => {
    beforeEach(() => {
      fileWatcherStore.addChange({
        fileKey: 'file1.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'file2.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'file3.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
    });

    it('should select a change by index', () => {
      fileWatcherStore.selectChange(1);

      const state = get(fileWatcherStore);
      expect(state.selectedChangeIndex).toBe(1);
    });

    it('should clamp to lower bound', () => {
      fileWatcherStore.selectChange(-5);

      const state = get(fileWatcherStore);
      expect(state.selectedChangeIndex).toBe(0);
    });

    it('should clamp to upper bound', () => {
      fileWatcherStore.selectChange(100);

      const state = get(fileWatcherStore);
      expect(state.selectedChangeIndex).toBe(2);
    });
  });

  describe('error handling', () => {
    it('should set error', () => {
      fileWatcherStore.setError('Watch failed');

      const state = get(fileWatcherStore);
      expect(state.error).toBe('Watch failed');
    });

    it('should clear error', () => {
      fileWatcherStore.setError('Watch failed');
      fileWatcherStore.clearError();

      const state = get(fileWatcherStore);
      expect(state.error).toBeNull();
    });
  });

  describe('hasPendingChange', () => {
    it('should return true when file has pending change', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      expect(fileWatcherStore.hasPendingChange('test.md')).toBe(true);
    });

    it('should return false when file has no pending change', () => {
      expect(fileWatcherStore.hasPendingChange('test.md')).toBe(false);
    });
  });

  describe('reset', () => {
    it('should reset to initial state', () => {
      fileWatcherStore.startWatching('/path/to/workspace');
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.setError('Some error');

      fileWatcherStore.reset();

      const state = get(fileWatcherStore);
      expect(state.isWatching).toBe(false);
      expect(state.workspaceRoot).toBeNull();
      expect(state.pendingChanges).toHaveLength(0);
      expect(state.showDialog).toBe(false);
      expect(state.selectedChangeIndex).toBe(0);
      expect(state.error).toBeNull();
    });
  });
});

describe('derived stores', () => {
  beforeEach(() => {
    fileWatcherStore.reset();
  });

  describe('hasPendingExternalChanges', () => {
    it('should be true when changes exist', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      expect(get(hasPendingExternalChanges)).toBe(true);
    });

    it('should be false when no changes', () => {
      expect(get(hasPendingExternalChanges)).toBe(false);
    });
  });

  describe('pendingChangeCount', () => {
    it('should return count of pending changes', () => {
      fileWatcherStore.addChange({
        fileKey: 'file1.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'file2.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      expect(get(pendingChangeCount)).toBe(2);
    });
  });

  describe('showExternalChangeDialog', () => {
    it('should reflect dialog visibility', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      expect(get(showExternalChangeDialog)).toBe(true);

      fileWatcherStore.closeDialog();

      expect(get(showExternalChangeDialog)).toBe(false);
    });
  });

  describe('pendingExternalChanges', () => {
    it('should return pending changes', () => {
      fileWatcherStore.addChange({
        fileKey: 'test.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      const changes = get(pendingExternalChanges);
      expect(changes).toHaveLength(1);
      expect(changes[0].fileKey).toBe('test.md');
    });
  });

  describe('selectedExternalChange', () => {
    it('should return selected change', () => {
      fileWatcherStore.addChange({
        fileKey: 'file1.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'file2.md',
        changeType: 'modify',
        timestamp: new Date(),
      });

      fileWatcherStore.selectChange(1);

      expect(get(selectedExternalChange)?.fileKey).toBe('file2.md');
    });

    it('should return null when no changes', () => {
      expect(get(selectedExternalChange)).toBeNull();
    });
  });

  describe('isFileWatching', () => {
    it('should reflect watching state', () => {
      expect(get(isFileWatching)).toBe(false);

      fileWatcherStore.startWatching('/path');

      expect(get(isFileWatching)).toBe(true);
    });
  });

  describe('changesByType', () => {
    it('should group changes by type', () => {
      fileWatcherStore.addChange({
        fileKey: 'modified1.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'modified2.md',
        changeType: 'modify',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'created.md',
        changeType: 'create',
        timestamp: new Date(),
      });
      fileWatcherStore.addChange({
        fileKey: 'deleted.md',
        changeType: 'delete',
        timestamp: new Date(),
      });

      const grouped = get(changesByType);

      expect(grouped.modified).toHaveLength(2);
      expect(grouped.created).toHaveLength(1);
      expect(grouped.deleted).toHaveLength(1);
    });

    it('should return empty arrays when no changes', () => {
      const grouped = get(changesByType);

      expect(grouped.modified).toHaveLength(0);
      expect(grouped.created).toHaveLength(0);
      expect(grouped.deleted).toHaveLength(0);
    });
  });
});
