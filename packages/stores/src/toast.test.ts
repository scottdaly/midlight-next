import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
  toastStore,
  toasts,
  visibleToasts,
  hiddenToastCount,
  hasToasts,
} from './toast';

describe('toastStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    toastStore.reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('show', () => {
    it('should add a toast with default success duration (3000ms)', () => {
      const id = toastStore.show('success', 'Success message');

      const state = get(toastStore);
      expect(state.toasts).toHaveLength(1);
      expect(state.toasts[0].type).toBe('success');
      expect(state.toasts[0].message).toBe('Success message');
      expect(state.toasts[0].duration).toBe(3000);
      expect(state.toasts[0].dismissible).toBe(false);
      expect(id).toMatch(/^toast-\d+$/);
    });

    it('should add a toast with default info duration (5000ms)', () => {
      toastStore.show('info', 'Info message');

      const state = get(toastStore);
      expect(state.toasts[0].duration).toBe(5000);
      expect(state.toasts[0].dismissible).toBe(true);
    });

    it('should add a toast with default warning duration (8000ms)', () => {
      toastStore.show('warning', 'Warning message');

      const state = get(toastStore);
      expect(state.toasts[0].duration).toBe(8000);
      expect(state.toasts[0].dismissible).toBe(true);
    });

    it('should add a toast with persistent error (0ms)', () => {
      toastStore.show('error', 'Error message');

      const state = get(toastStore);
      expect(state.toasts[0].duration).toBe(0);
      expect(state.toasts[0].dismissible).toBe(true);
    });

    it('should allow custom duration override', () => {
      toastStore.show('success', 'Custom duration', { duration: 10000 });

      const state = get(toastStore);
      expect(state.toasts[0].duration).toBe(10000);
    });

    it('should allow custom dismissible override', () => {
      toastStore.show('success', 'Dismissible', { dismissible: true });

      const state = get(toastStore);
      expect(state.toasts[0].dismissible).toBe(true);
    });

    it('should add action to toast', () => {
      const onClick = vi.fn();
      toastStore.show('info', 'With action', {
        action: { label: 'Undo', onClick },
      });

      const state = get(toastStore);
      expect(state.toasts[0].action).toBeDefined();
      expect(state.toasts[0].action?.label).toBe('Undo');
    });

    it('should generate unique IDs', () => {
      const id1 = toastStore.show('info', 'First');
      const id2 = toastStore.show('info', 'Second');

      expect(id1).not.toBe(id2);
    });

    it('should set createdAt timestamp', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      toastStore.show('info', 'With timestamp');

      const state = get(toastStore);
      expect(state.toasts[0].createdAt).toBe(now);
    });
  });

  describe('convenience methods', () => {
    it('success() should create success toast', () => {
      toastStore.success('Saved!');
      const state = get(toastStore);
      expect(state.toasts[0].type).toBe('success');
    });

    it('error() should create error toast with dismissible=true', () => {
      toastStore.error('Failed!');
      const state = get(toastStore);
      expect(state.toasts[0].type).toBe('error');
      expect(state.toasts[0].dismissible).toBe(true);
    });

    it('warning() should create warning toast', () => {
      toastStore.warning('Caution!');
      const state = get(toastStore);
      expect(state.toasts[0].type).toBe('warning');
    });

    it('info() should create info toast', () => {
      toastStore.info('FYI');
      const state = get(toastStore);
      expect(state.toasts[0].type).toBe('info');
    });
  });

  describe('auto-dismiss', () => {
    it('should auto-dismiss after duration', () => {
      toastStore.show('success', 'Auto dismiss');

      expect(get(toastStore).toasts).toHaveLength(1);

      // Fast-forward past the 3000ms duration
      vi.advanceTimersByTime(3000);

      expect(get(toastStore).toasts).toHaveLength(0);
    });

    it('should not auto-dismiss persistent toasts (duration=0)', () => {
      toastStore.show('error', 'Persistent');

      expect(get(toastStore).toasts).toHaveLength(1);

      vi.advanceTimersByTime(10000);

      expect(get(toastStore).toasts).toHaveLength(1);
    });
  });

  describe('dismiss', () => {
    it('should dismiss a specific toast', () => {
      const id = toastStore.show('info', 'To dismiss');

      expect(get(toastStore).toasts).toHaveLength(1);

      toastStore.dismiss(id);

      expect(get(toastStore).toasts).toHaveLength(0);
    });

    it('should clear timeout when dismissing', () => {
      const id = toastStore.show('info', 'To dismiss');

      toastStore.dismiss(id);

      // Advancing time should not cause issues
      vi.advanceTimersByTime(10000);

      expect(get(toastStore).toasts).toHaveLength(0);
    });
  });

  describe('dismissAll', () => {
    it('should dismiss all toasts', () => {
      toastStore.show('info', 'First');
      toastStore.show('success', 'Second');
      toastStore.show('warning', 'Third');

      expect(get(toastStore).toasts).toHaveLength(3);

      toastStore.dismissAll();

      expect(get(toastStore).toasts).toHaveLength(0);
    });

    it('should clear all timeouts', () => {
      toastStore.show('info', 'First');
      toastStore.show('success', 'Second');

      toastStore.dismissAll();

      // Advancing time should not cause issues
      vi.advanceTimersByTime(10000);

      expect(get(toastStore).toasts).toHaveLength(0);
    });
  });

  describe('pause/resume', () => {
    it('should pause auto-dismiss', () => {
      const id = toastStore.show('success', 'Pausable');

      // Advance partway through duration
      vi.advanceTimersByTime(1500);

      toastStore.pause(id);

      // Advance past original duration
      vi.advanceTimersByTime(3000);

      // Toast should still exist
      expect(get(toastStore).toasts).toHaveLength(1);
    });

    it('should resume with remaining time', () => {
      vi.setSystemTime(0);

      const id = toastStore.show('success', 'Resumable');
      const state = get(toastStore);
      expect(state.toasts[0].duration).toBe(3000);

      // Advance 1000ms
      vi.setSystemTime(1000);
      vi.advanceTimersByTime(1000);

      toastStore.pause(id);

      // Advance more time while paused
      vi.setSystemTime(2000);
      vi.advanceTimersByTime(1000);

      // Toast still exists
      expect(get(toastStore).toasts).toHaveLength(1);

      // Resume - remaining time should be 2000ms (3000 - 1000 elapsed)
      toastStore.resume(id);

      // Advance remaining time (2000ms)
      vi.advanceTimersByTime(2000);

      // Now it should be dismissed
      expect(get(toastStore).toasts).toHaveLength(0);
    });

    it('should dismiss immediately if time already expired on resume', () => {
      vi.setSystemTime(0);

      const id = toastStore.show('success', 'Expired');

      toastStore.pause(id);

      // Advance past duration while paused
      vi.setSystemTime(5000);

      toastStore.resume(id);

      // Should schedule immediate removal
      vi.advanceTimersByTime(0);

      expect(get(toastStore).toasts).toHaveLength(0);
    });

    it('should not schedule removal for persistent toasts', () => {
      const id = toastStore.show('error', 'Persistent');

      toastStore.pause(id);
      toastStore.resume(id);

      vi.advanceTimersByTime(10000);

      expect(get(toastStore).toasts).toHaveLength(1);
    });
  });

  describe('setMaxVisible', () => {
    it('should set max visible toasts', () => {
      toastStore.setMaxVisible(3);

      const state = get(toastStore);
      expect(state.maxVisible).toBe(3);
    });
  });

  describe('reset', () => {
    it('should reset to initial state', () => {
      toastStore.show('info', 'First');
      toastStore.show('info', 'Second');
      toastStore.setMaxVisible(3);

      toastStore.reset();

      const state = get(toastStore);
      expect(state.toasts).toHaveLength(0);
      expect(state.maxVisible).toBe(5);
    });

    it('should clear all timeouts on reset', () => {
      toastStore.show('success', 'First');
      toastStore.show('success', 'Second');

      toastStore.reset();

      vi.advanceTimersByTime(10000);

      expect(get(toastStore).toasts).toHaveLength(0);
    });
  });
});

describe('derived stores', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    toastStore.reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('toasts', () => {
    it('should return all toasts', () => {
      toastStore.show('info', 'First');
      toastStore.show('success', 'Second');

      expect(get(toasts)).toHaveLength(2);
    });
  });

  describe('visibleToasts', () => {
    it('should limit to maxVisible', () => {
      toastStore.setMaxVisible(2);

      toastStore.show('info', 'First');
      toastStore.show('info', 'Second');
      toastStore.show('info', 'Third');

      // Should show most recent
      const visible = get(visibleToasts);
      expect(visible).toHaveLength(2);
      expect(visible[0].message).toBe('Second');
      expect(visible[1].message).toBe('Third');
    });

    it('should show all when under maxVisible', () => {
      toastStore.setMaxVisible(5);

      toastStore.show('info', 'First');
      toastStore.show('info', 'Second');

      expect(get(visibleToasts)).toHaveLength(2);
    });
  });

  describe('hiddenToastCount', () => {
    it('should return number of hidden toasts', () => {
      toastStore.setMaxVisible(2);

      toastStore.show('info', 'First');
      toastStore.show('info', 'Second');
      toastStore.show('info', 'Third');

      expect(get(hiddenToastCount)).toBe(1);
    });

    it('should return 0 when no toasts hidden', () => {
      toastStore.setMaxVisible(5);

      toastStore.show('info', 'First');
      toastStore.show('info', 'Second');

      expect(get(hiddenToastCount)).toBe(0);
    });
  });

  describe('hasToasts', () => {
    it('should return true when toasts exist', () => {
      toastStore.show('info', 'First');
      expect(get(hasToasts)).toBe(true);
    });

    it('should return false when no toasts', () => {
      expect(get(hasToasts)).toBe(false);
    });
  });
});
