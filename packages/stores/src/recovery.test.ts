import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
  recoveryStore,
  hasPendingRecoveries,
  showRecoveryDialog,
  pendingRecoveries,
  isCheckingRecovery,
  scheduleWalWrite,
  cancelWalWrite,
  flushWalWrite,
  clearAllWalWrites,
  type RecoveryFile,
} from './recovery';

describe('recoveryStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    clearAllWalWrites(); // This also resets the store
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('initial state', () => {
    it('should have correct initial state', () => {
      const state = get(recoveryStore);

      expect(state.pendingRecoveries).toHaveLength(0);
      expect(state.showDialog).toBe(false);
      expect(state.isChecking).toBe(false);
      expect(state.activeWalFiles.size).toBe(0);
      expect(state.error).toBeNull();
    });
  });

  describe('startCheck', () => {
    it('should set isChecking to true', () => {
      recoveryStore.startCheck();

      const state = get(recoveryStore);
      expect(state.isChecking).toBe(true);
    });

    it('should clear error', () => {
      recoveryStore.checkFailed('Previous error');
      recoveryStore.startCheck();

      const state = get(recoveryStore);
      expect(state.error).toBeNull();
    });
  });

  describe('setPendingRecoveries', () => {
    const mockRecoveries: RecoveryFile[] = [
      {
        fileKey: 'test1.md',
        walContent: '# Test 1',
        walTime: '2024-01-01T00:00:00Z',
        workspaceRoot: '/path/to/workspace',
      },
      {
        fileKey: 'test2.md',
        walContent: '# Test 2',
        walTime: '2024-01-01T00:00:00Z',
        workspaceRoot: '/path/to/workspace',
      },
    ];

    it('should set pending recoveries', () => {
      recoveryStore.setPendingRecoveries(mockRecoveries);

      const state = get(recoveryStore);
      expect(state.pendingRecoveries).toHaveLength(2);
      expect(state.pendingRecoveries[0].fileKey).toBe('test1.md');
    });

    it('should set isChecking to false', () => {
      recoveryStore.startCheck();
      recoveryStore.setPendingRecoveries(mockRecoveries);

      const state = get(recoveryStore);
      expect(state.isChecking).toBe(false);
    });

    it('should show dialog when recoveries exist', () => {
      recoveryStore.setPendingRecoveries(mockRecoveries);

      const state = get(recoveryStore);
      expect(state.showDialog).toBe(true);
    });

    it('should not show dialog when no recoveries', () => {
      recoveryStore.setPendingRecoveries([]);

      const state = get(recoveryStore);
      expect(state.showDialog).toBe(false);
    });
  });

  describe('checkFailed', () => {
    it('should set error', () => {
      recoveryStore.checkFailed('Failed to check');

      const state = get(recoveryStore);
      expect(state.error).toBe('Failed to check');
    });

    it('should set isChecking to false', () => {
      recoveryStore.startCheck();
      recoveryStore.checkFailed('Failed');

      const state = get(recoveryStore);
      expect(state.isChecking).toBe(false);
    });
  });

  describe('removeRecovery', () => {
    beforeEach(() => {
      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test1.md',
          walContent: '# Test 1',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
        {
          fileKey: 'test2.md',
          walContent: '# Test 2',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);
    });

    it('should remove a recovery by fileKey', () => {
      recoveryStore.removeRecovery('test1.md');

      const state = get(recoveryStore);
      expect(state.pendingRecoveries).toHaveLength(1);
      expect(state.pendingRecoveries[0].fileKey).toBe('test2.md');
    });

    it('should hide dialog when all removed', () => {
      recoveryStore.removeRecovery('test1.md');
      recoveryStore.removeRecovery('test2.md');

      const state = get(recoveryStore);
      expect(state.showDialog).toBe(false);
    });

    it('should keep dialog open when some remain', () => {
      recoveryStore.removeRecovery('test1.md');

      const state = get(recoveryStore);
      expect(state.showDialog).toBe(true);
    });
  });

  describe('clearPendingRecoveries', () => {
    it('should clear all recoveries', () => {
      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test.md',
          walContent: '# Test',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);

      recoveryStore.clearPendingRecoveries();

      const state = get(recoveryStore);
      expect(state.pendingRecoveries).toHaveLength(0);
      expect(state.showDialog).toBe(false);
    });
  });

  describe('dialog visibility', () => {
    beforeEach(() => {
      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test.md',
          walContent: '# Test',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);
    });

    it('should close dialog', () => {
      recoveryStore.closeDialog();

      const state = get(recoveryStore);
      expect(state.showDialog).toBe(false);
      expect(state.pendingRecoveries).toHaveLength(1);
    });

    it('should open dialog when recoveries exist', () => {
      recoveryStore.closeDialog();
      recoveryStore.openDialog();

      const state = get(recoveryStore);
      expect(state.showDialog).toBe(true);
    });

    it('should not open dialog when no recoveries', () => {
      recoveryStore.clearPendingRecoveries();
      recoveryStore.openDialog();

      const state = get(recoveryStore);
      expect(state.showDialog).toBe(false);
    });
  });

  describe('active WAL files', () => {
    it('should add active WAL file', () => {
      recoveryStore.addActiveWal('test.md');

      const state = get(recoveryStore);
      expect(state.activeWalFiles.has('test.md')).toBe(true);
    });

    it('should remove active WAL file', () => {
      recoveryStore.addActiveWal('test.md');
      recoveryStore.removeActiveWal('test.md');

      const state = get(recoveryStore);
      expect(state.activeWalFiles.has('test.md')).toBe(false);
    });
  });

  describe('clearError', () => {
    it('should clear error', () => {
      recoveryStore.checkFailed('Error');
      recoveryStore.clearError();

      const state = get(recoveryStore);
      expect(state.error).toBeNull();
    });
  });

  describe('reset', () => {
    it('should reset to initial state', () => {
      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test.md',
          walContent: '# Test',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);
      recoveryStore.addActiveWal('test.md');
      recoveryStore.checkFailed('Error');

      recoveryStore.reset();

      const state = get(recoveryStore);
      expect(state.pendingRecoveries).toHaveLength(0);
      expect(state.showDialog).toBe(false);
      expect(state.isChecking).toBe(false);
      expect(state.activeWalFiles.size).toBe(0);
      expect(state.error).toBeNull();
    });
  });
});

describe('derived stores', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    clearAllWalWrites();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('hasPendingRecoveries', () => {
    it('should be true when recoveries exist', () => {
      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test.md',
          walContent: '# Test',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);

      expect(get(hasPendingRecoveries)).toBe(true);
    });

    it('should be false when no recoveries', () => {
      expect(get(hasPendingRecoveries)).toBe(false);
    });
  });

  describe('showRecoveryDialog', () => {
    it('should reflect dialog state', () => {
      expect(get(showRecoveryDialog)).toBe(false);

      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test.md',
          walContent: '# Test',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);

      expect(get(showRecoveryDialog)).toBe(true);
    });
  });

  describe('pendingRecoveries', () => {
    it('should return pending recoveries', () => {
      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test.md',
          walContent: '# Test',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);

      const recoveries = get(pendingRecoveries);
      expect(recoveries).toHaveLength(1);
      expect(recoveries[0].fileKey).toBe('test.md');
    });
  });

  describe('isCheckingRecovery', () => {
    it('should reflect checking state', () => {
      expect(get(isCheckingRecovery)).toBe(false);

      recoveryStore.startCheck();

      expect(get(isCheckingRecovery)).toBe(true);
    });
  });
});

describe('WAL write utilities', () => {
  let writeCallback: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    clearAllWalWrites();
    writeCallback = vi.fn().mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('scheduleWalWrite', () => {
    it('should debounce writes by 2000ms', async () => {
      scheduleWalWrite('test.md', 'content', writeCallback);

      expect(writeCallback).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1999);
      expect(writeCallback).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1);
      await vi.runOnlyPendingTimersAsync();

      expect(writeCallback).toHaveBeenCalledWith('test.md', 'content');
    });

    it('should add file to activeWalFiles', () => {
      scheduleWalWrite('test.md', 'content', writeCallback);

      const state = get(recoveryStore);
      expect(state.activeWalFiles.has('test.md')).toBe(true);
    });

    it('should skip if content unchanged', () => {
      scheduleWalWrite('test.md', 'content', writeCallback);
      scheduleWalWrite('test.md', 'content', writeCallback);

      vi.advanceTimersByTime(2000);

      // Only one call scheduled
      expect(writeCallback).toHaveBeenCalledTimes(1);
    });

    it('should reschedule on content change', async () => {
      scheduleWalWrite('test.md', 'content1', writeCallback);

      vi.advanceTimersByTime(1000);

      scheduleWalWrite('test.md', 'content2', writeCallback);

      vi.advanceTimersByTime(1000);
      expect(writeCallback).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1000);
      await vi.runOnlyPendingTimersAsync();

      expect(writeCallback).toHaveBeenCalledWith('test.md', 'content2');
    });

    it('should handle multiple files independently', async () => {
      scheduleWalWrite('file1.md', 'content1', writeCallback);
      scheduleWalWrite('file2.md', 'content2', writeCallback);

      vi.advanceTimersByTime(2000);
      await vi.runOnlyPendingTimersAsync();

      expect(writeCallback).toHaveBeenCalledTimes(2);
      expect(writeCallback).toHaveBeenCalledWith('file1.md', 'content1');
      expect(writeCallback).toHaveBeenCalledWith('file2.md', 'content2');
    });
  });

  describe('cancelWalWrite', () => {
    it('should cancel pending write', async () => {
      scheduleWalWrite('test.md', 'content', writeCallback);

      cancelWalWrite('test.md');

      vi.advanceTimersByTime(2000);
      await vi.runOnlyPendingTimersAsync();

      expect(writeCallback).not.toHaveBeenCalled();
    });

    it('should remove from activeWalFiles', () => {
      scheduleWalWrite('test.md', 'content', writeCallback);

      cancelWalWrite('test.md');

      const state = get(recoveryStore);
      expect(state.activeWalFiles.has('test.md')).toBe(false);
    });

    it('should handle non-existent file', () => {
      // Should not throw
      cancelWalWrite('nonexistent.md');
    });
  });

  describe('flushWalWrite', () => {
    it('should write immediately', async () => {
      scheduleWalWrite('test.md', 'content', writeCallback);

      await flushWalWrite('test.md', writeCallback);

      expect(writeCallback).toHaveBeenCalledWith('test.md', 'content');
    });

    it('should cancel scheduled timeout', async () => {
      scheduleWalWrite('test.md', 'content', writeCallback);

      await flushWalWrite('test.md', writeCallback);
      writeCallback.mockClear();

      vi.advanceTimersByTime(2000);
      await vi.runOnlyPendingTimersAsync();

      expect(writeCallback).not.toHaveBeenCalled();
    });

    it('should remove from activeWalFiles', async () => {
      scheduleWalWrite('test.md', 'content', writeCallback);

      await flushWalWrite('test.md', writeCallback);

      const state = get(recoveryStore);
      expect(state.activeWalFiles.has('test.md')).toBe(false);
    });

    it('should handle non-existent file', async () => {
      // Should not throw
      await flushWalWrite('nonexistent.md', writeCallback);
      expect(writeCallback).not.toHaveBeenCalled();
    });

    it('should remove from activeWalFiles even if callback fails', async () => {
      const failingCallback = vi.fn().mockRejectedValue(new Error('Write failed'));

      scheduleWalWrite('test.md', 'content', failingCallback);

      await expect(flushWalWrite('test.md', failingCallback)).rejects.toThrow('Write failed');

      const state = get(recoveryStore);
      expect(state.activeWalFiles.has('test.md')).toBe(false);
    });
  });

  describe('clearAllWalWrites', () => {
    it('should cancel all pending writes', async () => {
      scheduleWalWrite('file1.md', 'content1', writeCallback);
      scheduleWalWrite('file2.md', 'content2', writeCallback);

      clearAllWalWrites();

      vi.advanceTimersByTime(2000);
      await vi.runOnlyPendingTimersAsync();

      expect(writeCallback).not.toHaveBeenCalled();
    });

    it('should reset store state', () => {
      recoveryStore.setPendingRecoveries([
        {
          fileKey: 'test.md',
          walContent: '# Test',
          walTime: '2024-01-01T00:00:00Z',
          workspaceRoot: '/path',
        },
      ]);
      scheduleWalWrite('test.md', 'content', writeCallback);

      clearAllWalWrites();

      const state = get(recoveryStore);
      expect(state.pendingRecoveries).toHaveLength(0);
      expect(state.activeWalFiles.size).toBe(0);
    });
  });
});
