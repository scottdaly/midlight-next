// @midlight/stores/ui - UI state management

import { writable, derived } from 'svelte/store';

export type RightPanelMode = 'chat' | 'versions' | 'pending' | 'context' | null;

export interface UIState {
  rightPanelMode: RightPanelMode;
  leftSidebarOpen: boolean;
}

const initialState: UIState = {
  rightPanelMode: 'chat', // Default to chat panel open
  leftSidebarOpen: true,
};

function createUIStore() {
  const { subscribe, set, update } = writable<UIState>(initialState);

  return {
    subscribe,

    /**
     * Sets the right panel mode
     */
    setRightPanelMode(mode: RightPanelMode) {
      update((s) => ({ ...s, rightPanelMode: mode }));
    },

    /**
     * Toggles the right panel (opens to chat if closed, closes if open)
     */
    toggleRightPanel() {
      update((s) => ({
        ...s,
        rightPanelMode: s.rightPanelMode === null ? 'chat' : null,
      }));
    },

    /**
     * Opens or toggles to a specific panel mode
     * If already showing that mode, closes the panel
     */
    togglePanelMode(mode: 'chat' | 'versions' | 'pending' | 'context') {
      update((s) => ({
        ...s,
        rightPanelMode: s.rightPanelMode === mode ? null : mode,
      }));
    },

    /**
     * Toggles the left sidebar
     */
    toggleLeftSidebar() {
      update((s) => ({ ...s, leftSidebarOpen: !s.leftSidebarOpen }));
    },

    /**
     * Sets left sidebar visibility
     */
    setLeftSidebarOpen(open: boolean) {
      update((s) => ({ ...s, leftSidebarOpen: open }));
    },

    /**
     * Resets to initial state
     */
    reset() {
      set(initialState);
    },
  };
}

export const ui = createUIStore();

// Derived stores
export const rightPanelMode = derived(ui, ($ui) => $ui.rightPanelMode);
export const isRightPanelOpen = derived(ui, ($ui) => $ui.rightPanelMode !== null);
export const leftSidebarOpen = derived(ui, ($ui) => $ui.leftSidebarOpen);
