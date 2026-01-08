import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
  shortcuts,
  allShortcuts,
  shortcutsByCategory,
  hasCustomizations,
  matchesShortcut,
  getDisplayKey,
  getModifierName,
} from './shortcuts';

// Mock localStorage
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] || null,
    setItem: (key: string, value: string) => {
      store[key] = value;
    },
    removeItem: (key: string) => {
      delete store[key];
    },
    clear: () => {
      store = {};
    },
  };
})();

Object.defineProperty(globalThis, 'localStorage', {
  value: localStorageMock,
  writable: true,
});

describe('shortcuts', () => {
  beforeEach(() => {
    // Clear localStorage
    localStorageMock.clear();

    // Reset shortcuts store - unregister all and reset customizations
    const state = get(shortcuts);
    for (const id of state.shortcuts.keys()) {
      shortcuts.unregister(id);
    }
    shortcuts.resetAllToDefaults();
    shortcuts.enable();
  });

  describe('register/unregister', () => {
    it('should register a shortcut', () => {
      const action = vi.fn();
      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action,
      });

      const state = get(shortcuts);
      expect(state.shortcuts.has('test-shortcut')).toBe(true);
      expect(state.shortcuts.get('test-shortcut')?.keys).toBe('mod+k');
    });

    it('should set default values for preventDefault and customizable', () => {
      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action: vi.fn(),
      });

      const state = get(shortcuts);
      const shortcut = state.shortcuts.get('test-shortcut');
      expect(shortcut?.preventDefault).toBe(true);
      expect(shortcut?.customizable).toBe(true);
    });

    it('should register multiple shortcuts at once', () => {
      shortcuts.registerAll([
        { id: 's1', keys: 'mod+1', description: 'One', category: 'navigation', action: vi.fn() },
        { id: 's2', keys: 'mod+2', description: 'Two', category: 'navigation', action: vi.fn() },
      ]);

      const state = get(shortcuts);
      expect(state.shortcuts.size).toBe(2);
      expect(state.shortcuts.has('s1')).toBe(true);
      expect(state.shortcuts.has('s2')).toBe(true);
    });

    it('should unregister a shortcut', () => {
      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action: vi.fn(),
      });

      shortcuts.unregister('test-shortcut');

      const state = get(shortcuts);
      expect(state.shortcuts.has('test-shortcut')).toBe(false);
    });
  });

  describe('enable/disable', () => {
    it('should start enabled', () => {
      const state = get(shortcuts);
      expect(state.enabled).toBe(true);
    });

    it('should disable shortcuts', () => {
      shortcuts.disable();
      const state = get(shortcuts);
      expect(state.enabled).toBe(false);
    });

    it('should enable shortcuts', () => {
      shortcuts.disable();
      shortcuts.enable();
      const state = get(shortcuts);
      expect(state.enabled).toBe(true);
    });
  });

  describe('customize/reset', () => {
    beforeEach(() => {
      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action: vi.fn(),
      });
    });

    it('should customize a shortcut', () => {
      shortcuts.customize('test-shortcut', 'mod+shift+k');

      const state = get(shortcuts);
      expect(state.customizations['test-shortcut']).toBe('mod+shift+k');
    });

    it('should reset a shortcut to default', () => {
      shortcuts.customize('test-shortcut', 'mod+shift+k');
      shortcuts.resetToDefault('test-shortcut');

      const state = get(shortcuts);
      expect(state.customizations['test-shortcut']).toBeUndefined();
    });

    it('should reset all shortcuts to defaults', () => {
      shortcuts.customize('test-shortcut', 'mod+shift+k');
      shortcuts.resetAllToDefaults();

      const state = get(shortcuts);
      expect(Object.keys(state.customizations)).toHaveLength(0);
    });

    it('should get effective keys (customized)', () => {
      shortcuts.customize('test-shortcut', 'mod+shift+k');

      const keys = shortcuts.getEffectiveKeys('test-shortcut');
      expect(keys).toBe('mod+shift+k');
    });

    it('should get effective keys (default when no customization)', () => {
      // Ensure no customization exists
      shortcuts.resetToDefault('test-shortcut');

      const keys = shortcuts.getEffectiveKeys('test-shortcut');
      expect(keys).toBe('mod+k');
    });
  });

  describe('handleKeyDown', () => {
    // Detect platform to know which modifier to use
    const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0;

    it('should execute matching shortcut', () => {
      const action = vi.fn();
      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action,
      });

      const event = new KeyboardEvent('keydown', {
        key: 'k',
        metaKey: isMac,
        ctrlKey: !isMac,
      });

      const prevented = vi.spyOn(event, 'preventDefault');
      const handled = shortcuts.handleKeyDown(event);

      expect(handled).toBe(true);
      expect(action).toHaveBeenCalled();
      expect(prevented).toHaveBeenCalled();
    });

    it('should not execute when disabled', () => {
      const action = vi.fn();
      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action,
      });

      shortcuts.disable();

      const event = new KeyboardEvent('keydown', {
        key: 'k',
        metaKey: isMac,
        ctrlKey: !isMac,
      });

      const handled = shortcuts.handleKeyDown(event);

      expect(handled).toBe(false);
      expect(action).not.toHaveBeenCalled();
    });

    it('should respect when() condition', () => {
      const action = vi.fn();
      let shouldExecute = false;

      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action,
        when: () => shouldExecute,
      });

      const event = new KeyboardEvent('keydown', {
        key: 'k',
        metaKey: isMac,
        ctrlKey: !isMac,
      });

      // Should not execute when condition is false
      shortcuts.handleKeyDown(event);
      expect(action).not.toHaveBeenCalled();

      // Should execute when condition is true
      shouldExecute = true;
      shortcuts.handleKeyDown(event);
      expect(action).toHaveBeenCalled();
    });

    it('should use customized keys', () => {
      const action = vi.fn();
      shortcuts.register({
        id: 'test-shortcut',
        keys: 'mod+k',
        description: 'Test shortcut',
        category: 'editing',
        action,
      });

      shortcuts.customize('test-shortcut', 'mod+shift+k');

      // Original keys should not work
      const event1 = new KeyboardEvent('keydown', {
        key: 'k',
        metaKey: isMac,
        ctrlKey: !isMac,
        shiftKey: false,
      });
      shortcuts.handleKeyDown(event1);
      expect(action).not.toHaveBeenCalled();

      // Customized keys should work
      const event2 = new KeyboardEvent('keydown', {
        key: 'k',
        metaKey: isMac,
        ctrlKey: !isMac,
        shiftKey: true,
      });
      shortcuts.handleKeyDown(event2);
      expect(action).toHaveBeenCalled();
    });
  });

  describe('derived stores', () => {
    it('should update allShortcuts when shortcuts change', () => {
      shortcuts.register({
        id: 's1',
        keys: 'mod+1',
        description: 'One',
        category: 'navigation',
        action: vi.fn(),
      });

      const all = get(allShortcuts);
      expect(all).toHaveLength(1);
      expect(all[0].id).toBe('s1');
    });

    it('should group shortcuts by category', () => {
      shortcuts.registerAll([
        { id: 'nav1', keys: 'mod+1', description: 'Nav 1', category: 'navigation', action: vi.fn() },
        { id: 'nav2', keys: 'mod+2', description: 'Nav 2', category: 'navigation', action: vi.fn() },
        { id: 'edit1', keys: 'mod+e', description: 'Edit 1', category: 'editing', action: vi.fn() },
      ]);

      const grouped = get(shortcutsByCategory);
      expect(grouped.navigation).toHaveLength(2);
      expect(grouped.editing).toHaveLength(1);
      expect(grouped.file).toHaveLength(0);
    });

    it('should track hasCustomizations', () => {
      shortcuts.register({
        id: 'test',
        keys: 'mod+k',
        description: 'Test',
        category: 'editing',
        action: vi.fn(),
      });

      // Should start with no customizations
      expect(get(hasCustomizations)).toBe(false);

      shortcuts.customize('test', 'mod+shift+k');
      expect(get(hasCustomizations)).toBe(true);

      shortcuts.resetToDefault('test');
      expect(get(hasCustomizations)).toBe(false);
    });
  });
});

describe('matchesShortcut', () => {
  // Detect platform - this matches how the module detects it
  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0;

  it('should match mod+key using platform-specific modifier', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      metaKey: isMac,
      ctrlKey: !isMac,
    });
    expect(matchesShortcut(event, 'mod+k')).toBe(true);
  });

  it('should match mod+shift+key', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'p',
      metaKey: isMac,
      ctrlKey: !isMac,
      shiftKey: true,
    });
    expect(matchesShortcut(event, 'mod+shift+p')).toBe(true);
  });

  it('should match mod+alt+key', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'l',
      metaKey: isMac,
      ctrlKey: !isMac,
      altKey: true,
    });
    expect(matchesShortcut(event, 'mod+alt+l')).toBe(true);
  });

  it('should not match when modifiers differ', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      metaKey: isMac,
      ctrlKey: !isMac,
      shiftKey: false,
    });
    expect(matchesShortcut(event, 'mod+shift+k')).toBe(false);
  });

  it('should be case insensitive for keys', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'K',
      metaKey: isMac,
      ctrlKey: !isMac,
    });
    expect(matchesShortcut(event, 'mod+k')).toBe(true);
  });

  it('should not match when wrong modifier key used', () => {
    // On Mac, ctrlKey should not trigger mod+k (metaKey should)
    // On non-Mac, metaKey should not trigger mod+k (ctrlKey should)
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      metaKey: !isMac, // Wrong modifier
      ctrlKey: isMac,  // Wrong modifier
    });
    expect(matchesShortcut(event, 'mod+k')).toBe(false);
  });
});

describe('getDisplayKey', () => {
  it('should format key combination to include the main key', () => {
    const display = getDisplayKey('mod+k');
    expect(display.includes('K')).toBe(true);
  });

  it('should handle shift modifier', () => {
    const display = getDisplayKey('mod+shift+p');
    expect(display.includes('P')).toBe(true);
  });

  it('should remove plus signs', () => {
    const display = getDisplayKey('mod+shift+k');
    expect(display.includes('+')).toBe(false);
  });

  it('should uppercase the result', () => {
    const display = getDisplayKey('mod+k');
    expect(display).toBe(display.toUpperCase());
  });
});

describe('getModifierName', () => {
  it('should return platform-specific modifier name', () => {
    const name = getModifierName();
    // Should be either the Mac symbol or 'Ctrl'
    expect(['Ctrl', '\u2318']).toContain(name);
  });
});
