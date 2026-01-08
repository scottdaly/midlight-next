// @midlight/stores/toast - Toast notification state management

import { writable, derived } from 'svelte/store';

// ============================================================================
// Types
// ============================================================================

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration: number; // 0 = persistent until dismissed
  action?: ToastAction;
  dismissible: boolean;
  createdAt: number;
}

export interface ToastState {
  toasts: Toast[];
  maxVisible: number;
}

// ============================================================================
// Default durations by type (in ms)
// ============================================================================

const DEFAULTS: Record<ToastType, { duration: number; dismissible: boolean }> = {
  success: { duration: 3000, dismissible: false },
  info: { duration: 5000, dismissible: true },
  warning: { duration: 8000, dismissible: true },
  error: { duration: 0, dismissible: true }, // Persistent until dismissed
};

// ============================================================================
// Toast Store
// ============================================================================

const initialState: ToastState = {
  toasts: [],
  maxVisible: 5,
};

let toastCounter = 0;

function createToastStore() {
  const { subscribe, set, update } = writable<ToastState>(initialState);

  // Track timeouts for auto-dismiss
  const timeouts = new Map<string, ReturnType<typeof setTimeout>>();

  function scheduleRemoval(id: string, duration: number) {
    if (duration <= 0) return; // Don't auto-remove persistent toasts

    const timeout = setTimeout(() => {
      remove(id);
    }, duration);

    timeouts.set(id, timeout);
  }

  function cancelRemoval(id: string) {
    const timeout = timeouts.get(id);
    if (timeout) {
      clearTimeout(timeout);
      timeouts.delete(id);
    }
  }

  function remove(id: string) {
    cancelRemoval(id);
    update((s) => ({
      ...s,
      toasts: s.toasts.filter((t) => t.id !== id),
    }));
  }

  return {
    subscribe,

    /**
     * Show a toast notification
     */
    show(
      type: ToastType,
      message: string,
      options?: {
        duration?: number;
        dismissible?: boolean;
        action?: ToastAction;
      }
    ): string {
      const id = `toast-${++toastCounter}`;
      const defaults = DEFAULTS[type];

      const toast: Toast = {
        id,
        type,
        message,
        duration: options?.duration ?? defaults.duration,
        dismissible: options?.dismissible ?? defaults.dismissible,
        action: options?.action,
        createdAt: Date.now(),
      };

      update((s) => ({
        ...s,
        toasts: [...s.toasts, toast],
      }));

      scheduleRemoval(id, toast.duration);

      return id;
    },

    /**
     * Show a success toast
     */
    success(message: string, options?: { duration?: number; action?: ToastAction }): string {
      return this.show('success', message, options);
    },

    /**
     * Show an error toast
     */
    error(message: string, options?: { duration?: number; action?: ToastAction }): string {
      return this.show('error', message, { ...options, dismissible: true });
    },

    /**
     * Show a warning toast
     */
    warning(message: string, options?: { duration?: number; action?: ToastAction }): string {
      return this.show('warning', message, options);
    },

    /**
     * Show an info toast
     */
    info(message: string, options?: { duration?: number; action?: ToastAction }): string {
      return this.show('info', message, options);
    },

    /**
     * Dismiss a specific toast
     */
    dismiss(id: string) {
      remove(id);
    },

    /**
     * Dismiss all toasts
     */
    dismissAll() {
      // Clear all timeouts
      for (const [id] of timeouts) {
        cancelRemoval(id);
      }
      update((s) => ({ ...s, toasts: [] }));
    },

    /**
     * Pause auto-dismiss (e.g., when hovering)
     */
    pause(id: string) {
      cancelRemoval(id);
    },

    /**
     * Resume auto-dismiss with remaining time
     */
    resume(id: string) {
      update((s) => {
        const toast = s.toasts.find((t) => t.id === id);
        if (!toast || toast.duration <= 0) return s;

        const elapsed = Date.now() - toast.createdAt;
        const remaining = Math.max(0, toast.duration - elapsed);

        if (remaining > 0) {
          scheduleRemoval(id, remaining);
        } else {
          // Time already expired, remove immediately
          setTimeout(() => remove(id), 0);
        }

        return s;
      });
    },

    /**
     * Set maximum visible toasts
     */
    setMaxVisible(max: number) {
      update((s) => ({ ...s, maxVisible: max }));
    },

    /**
     * Reset the store
     */
    reset() {
      // Clear all timeouts
      for (const [id] of timeouts) {
        cancelRemoval(id);
      }
      set(initialState);
    },
  };
}

export const toastStore = createToastStore();

// ============================================================================
// Derived Stores
// ============================================================================

/** All toasts */
export const toasts = derived(toastStore, ($store) => $store.toasts);

/** Visible toasts (limited by maxVisible) */
export const visibleToasts = derived(toastStore, ($store) =>
  $store.toasts.slice(-$store.maxVisible)
);

/** Hidden toast count (collapsed) */
export const hiddenToastCount = derived(toastStore, ($store) =>
  Math.max(0, $store.toasts.length - $store.maxVisible)
);

/** Whether there are any toasts */
export const hasToasts = derived(toastStore, ($store) => $store.toasts.length > 0);
