// @midlight/stores/shortcuts - Keyboard shortcut management

import { writable, derived } from 'svelte/store';

// ============================================================================
// Types
// ============================================================================

export interface Shortcut {
  /** Unique identifier for the shortcut */
  id: string;
  /** Key combination (e.g., "mod+k", "mod+shift+p") - "mod" = Cmd on Mac, Ctrl on Windows */
  keys: string;
  /** Human-readable description */
  description: string;
  /** Category for grouping in UI */
  category: ShortcutCategory;
  /** Action to execute */
  action: () => void | Promise<void>;
  /** Only active when this returns true */
  when?: () => boolean;
  /** Prevent default browser behavior (default: true) */
  preventDefault?: boolean;
  /** Allow the shortcut to be customized (default: true) */
  customizable?: boolean;
}

export type ShortcutCategory =
  | 'navigation'
  | 'editing'
  | 'file'
  | 'view'
  | 'ai'
  | 'other';

export interface ShortcutState {
  /** Registered shortcuts */
  shortcuts: Map<string, Shortcut>;
  /** User customizations (shortcut id -> custom keys) */
  customizations: Record<string, string>;
  /** Whether shortcuts are enabled (disabled during text input) */
  enabled: boolean;
}

// ============================================================================
// Platform Detection
// ============================================================================

const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0;

/**
 * Get the platform-specific modifier key name
 */
export function getModifierName(): string {
  return isMac ? '⌘' : 'Ctrl';
}

/**
 * Get human-readable key name for display
 */
export function getDisplayKey(keys: string): string {
  return keys
    .replace(/mod/g, isMac ? '⌘' : 'Ctrl')
    .replace(/alt/g, isMac ? '⌥' : 'Alt')
    .replace(/shift/g, isMac ? '⇧' : 'Shift')
    .replace(/\+/g, '')
    .toUpperCase();
}

/**
 * Parse a key combination string into components
 */
function parseKeys(keys: string): { mod: boolean; alt: boolean; shift: boolean; key: string } {
  const parts = keys.toLowerCase().split('+');
  const key = parts[parts.length - 1];

  return {
    mod: parts.includes('mod'),
    alt: parts.includes('alt'),
    shift: parts.includes('shift'),
    key,
  };
}

/**
 * Check if a keyboard event matches a key combination
 */
export function matchesShortcut(event: KeyboardEvent, keys: string): boolean {
  const parsed = parseKeys(keys);

  // Check modifier keys
  const modKey = isMac ? event.metaKey : event.ctrlKey;
  if (parsed.mod !== modKey) return false;
  if (parsed.alt !== event.altKey) return false;
  if (parsed.shift !== event.shiftKey) return false;

  // Check the main key
  return event.key.toLowerCase() === parsed.key;
}

// ============================================================================
// Shortcut Store
// ============================================================================

const initialState: ShortcutState = {
  shortcuts: new Map(),
  customizations: loadCustomizations(),
  enabled: true,
};

function loadCustomizations(): Record<string, string> {
  if (typeof localStorage === 'undefined') return {};
  try {
    const stored = localStorage.getItem('midlight-shortcuts');
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

function saveCustomizations(customizations: Record<string, string>) {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem('midlight-shortcuts', JSON.stringify(customizations));
  } catch {
    // Ignore storage errors
  }
}

function createShortcutStore() {
  const { subscribe, set, update } = writable<ShortcutState>(initialState);

  return {
    subscribe,

    /**
     * Register a shortcut
     */
    register(shortcut: Shortcut) {
      update((s) => {
        const newShortcuts = new Map(s.shortcuts);
        newShortcuts.set(shortcut.id, {
          ...shortcut,
          preventDefault: shortcut.preventDefault ?? true,
          customizable: shortcut.customizable ?? true,
        });
        return { ...s, shortcuts: newShortcuts };
      });
    },

    /**
     * Register multiple shortcuts at once
     */
    registerAll(shortcuts: Shortcut[]) {
      update((s) => {
        const newShortcuts = new Map(s.shortcuts);
        for (const shortcut of shortcuts) {
          newShortcuts.set(shortcut.id, {
            ...shortcut,
            preventDefault: shortcut.preventDefault ?? true,
            customizable: shortcut.customizable ?? true,
          });
        }
        return { ...s, shortcuts: newShortcuts };
      });
    },

    /**
     * Unregister a shortcut
     */
    unregister(id: string) {
      update((s) => {
        const newShortcuts = new Map(s.shortcuts);
        newShortcuts.delete(id);
        return { ...s, shortcuts: newShortcuts };
      });
    },

    /**
     * Get the effective keys for a shortcut (considering customizations)
     */
    getEffectiveKeys(id: string): string | null {
      let result: string | null = null;
      subscribe((s) => {
        const shortcut = s.shortcuts.get(id);
        if (shortcut) {
          result = s.customizations[id] || shortcut.keys;
        }
      })();
      return result;
    },

    /**
     * Customize a shortcut's keys
     */
    customize(id: string, keys: string) {
      update((s) => {
        const newCustomizations = { ...s.customizations, [id]: keys };
        saveCustomizations(newCustomizations);
        return { ...s, customizations: newCustomizations };
      });
    },

    /**
     * Reset a shortcut to its default keys
     */
    resetToDefault(id: string) {
      update((s) => {
        const { [id]: _, ...newCustomizations } = s.customizations;
        saveCustomizations(newCustomizations);
        return { ...s, customizations: newCustomizations };
      });
    },

    /**
     * Reset all shortcuts to defaults
     */
    resetAllToDefaults() {
      update((s) => {
        saveCustomizations({});
        return { ...s, customizations: {} };
      });
    },

    /**
     * Enable shortcuts (default state)
     */
    enable() {
      update((s) => ({ ...s, enabled: true }));
    },

    /**
     * Disable shortcuts (e.g., during text input)
     */
    disable() {
      update((s) => ({ ...s, enabled: false }));
    },

    /**
     * Handle a keyboard event, executing matching shortcuts
     * Returns true if a shortcut was executed
     */
    handleKeyDown(event: KeyboardEvent): boolean {
      let handled = false;

      subscribe((s) => {
        if (!s.enabled) return;

        for (const [id, shortcut] of s.shortcuts) {
          // Get effective keys (customized or default)
          const effectiveKeys = s.customizations[id] || shortcut.keys;

          // Check if this shortcut matches
          if (!matchesShortcut(event, effectiveKeys)) continue;

          // Check condition
          if (shortcut.when && !shortcut.when()) continue;

          // Execute the action
          if (shortcut.preventDefault) {
            event.preventDefault();
          }

          try {
            shortcut.action();
          } catch (error) {
            console.error(`Shortcut ${id} failed:`, error);
          }

          handled = true;
          break; // Only execute one shortcut per event
        }
      })();

      return handled;
    },
  };
}

export const shortcuts = createShortcutStore();

// ============================================================================
// Derived Stores
// ============================================================================

/** All registered shortcuts as an array */
export const allShortcuts = derived(
  shortcuts,
  ($shortcuts) => Array.from($shortcuts.shortcuts.values())
);

/** Shortcuts grouped by category */
export const shortcutsByCategory = derived(
  shortcuts,
  ($shortcuts) => {
    const groups: Record<ShortcutCategory, Shortcut[]> = {
      navigation: [],
      editing: [],
      file: [],
      view: [],
      ai: [],
      other: [],
    };

    for (const shortcut of $shortcuts.shortcuts.values()) {
      groups[shortcut.category].push(shortcut);
    }

    return groups;
  }
);

/** Whether any shortcuts have been customized */
export const hasCustomizations = derived(
  shortcuts,
  ($shortcuts) => Object.keys($shortcuts.customizations).length > 0
);
