// @midlight/stores/context - Context update state management

import { writable, derived, get } from 'svelte/store';
import type {
  ContextUpdate,
  ContextUpdateResult,
  ParsedContext,
} from '@midlight/core';
import {
  buildExtractionPrompt,
  parseExtractionResponse,
  applyContextUpdates,
  summarizeUpdates,
  filterSafeUpdates,
  createContextDiff,
} from '@midlight/core';

/**
 * A pending context update waiting for user approval
 */
export interface PendingContextUpdate {
  id: string;
  projectPath: string;
  updates: ContextUpdate[];
  originalContext: string;
  newContext: string;
  summary: string;
  diffs: { section: string; oldValue: string; newValue: string }[];
  createdAt: string;
}

/**
 * History entry for undo functionality
 */
export interface ContextUpdateHistoryEntry {
  id: string;
  projectPath: string;
  previousContext: string;
  newContext: string;
  updates: ContextUpdate[];
  appliedAt: string;
}

export interface ContextUpdateState {
  pendingUpdate: PendingContextUpdate | null;
  showConfirmDialog: boolean;
  isExtracting: boolean;
  isApplying: boolean;
  error: string | null;
  history: ContextUpdateHistoryEntry[];
  lastUpdateAt: string | null;
}

const initialState: ContextUpdateState = {
  pendingUpdate: null,
  showConfirmDialog: false,
  isExtracting: false,
  isApplying: false,
  error: null,
  history: [],
  lastUpdateAt: null,
};

// Types for injected functions
export type ContextLoader = (projectPath: string) => Promise<string | null>;
export type ContextSaver = (projectPath: string, content: string) => Promise<void>;
export type ExtractionLLMCall = (prompt: string) => Promise<string>;

function createContextUpdateStore() {
  const { subscribe, set, update } = writable<ContextUpdateState>(initialState);

  // Injected functions (set by platform layer)
  let contextLoader: ContextLoader | null = null;
  let contextSaver: ContextSaver | null = null;
  let extractionLLMCall: ExtractionLLMCall | null = null;

  // Settings (synced from settings store)
  let autoUpdate = true;
  let askBeforeUpdating = false;
  let showNotifications = false;

  return {
    subscribe,

    /**
     * Sets the context loader function
     */
    setContextLoader(loader: ContextLoader) {
      contextLoader = loader;
    },

    /**
     * Sets the context saver function
     */
    setContextSaver(saver: ContextSaver) {
      contextSaver = saver;
    },

    /**
     * Sets the LLM call function for extraction
     */
    setExtractionLLMCall(llmCall: ExtractionLLMCall) {
      extractionLLMCall = llmCall;
    },

    /**
     * Updates settings from the settings store
     */
    updateSettings(settings: {
      autoUpdate: boolean;
      askBeforeUpdating: boolean;
      showNotifications: boolean;
    }) {
      autoUpdate = settings.autoUpdate;
      askBeforeUpdating = settings.askBeforeUpdating;
      showNotifications = settings.showNotifications;
    },

    /**
     * Evaluates a conversation for potential context updates
     * Called after each AI response
     */
    async evaluateForUpdates(
      projectPath: string,
      userMessage: string,
      assistantResponse: string
    ): Promise<ContextUpdateResult | null> {
      if (!autoUpdate) {
        return null;
      }

      if (!contextLoader || !extractionLLMCall) {
        console.warn('[ContextUpdate] Loaders not configured');
        return null;
      }

      update((s) => ({ ...s, isExtracting: true, error: null }));

      try {
        // Load current context
        const currentContext = await contextLoader(projectPath);
        if (!currentContext) {
          update((s) => ({ ...s, isExtracting: false }));
          return null;
        }

        // Build extraction prompt
        const prompt = buildExtractionPrompt(currentContext, userMessage, assistantResponse);

        // Call LLM for extraction
        const response = await extractionLLMCall(prompt);

        // Parse the response
        const result = parseExtractionResponse(response);

        update((s) => ({ ...s, isExtracting: false }));

        if (!result.shouldUpdate || result.updates.length === 0) {
          return null;
        }

        // If ask before updating is enabled, show confirmation
        if (askBeforeUpdating) {
          // Filter to safe updates only
          const safeUpdates = filterSafeUpdates(result.updates);
          if (safeUpdates.length === 0) {
            return null;
          }

          // Generate new context
          const newContext = applyContextUpdates(currentContext, safeUpdates);

          // Create pending update
          const pendingUpdate: PendingContextUpdate = {
            id: `update-${Date.now()}`,
            projectPath,
            updates: safeUpdates,
            originalContext: currentContext,
            newContext,
            summary: summarizeUpdates(safeUpdates),
            diffs: createContextDiff(currentContext, newContext),
            createdAt: new Date().toISOString(),
          };

          update((s) => ({
            ...s,
            pendingUpdate,
            showConfirmDialog: true,
          }));

          return result;
        }

        // Auto-apply safe updates
        const safeUpdates = filterSafeUpdates(result.updates);
        if (safeUpdates.length > 0) {
          await this.applyUpdates(projectPath, currentContext, safeUpdates);
        }

        return result;
      } catch (error) {
        console.error('[ContextUpdate] Extraction failed:', error);
        update((s) => ({
          ...s,
          isExtracting: false,
          error: error instanceof Error ? error.message : String(error),
        }));
        return null;
      }
    },

    /**
     * Applies context updates
     */
    async applyUpdates(
      projectPath: string,
      originalContext: string,
      updates: ContextUpdate[]
    ): Promise<boolean> {
      if (!contextSaver) {
        console.warn('[ContextUpdate] Saver not configured');
        return false;
      }

      update((s) => ({ ...s, isApplying: true, error: null }));

      try {
        const newContext = applyContextUpdates(originalContext, updates);

        // Save the updated context
        await contextSaver(projectPath, newContext);

        // Add to history for undo
        const historyEntry: ContextUpdateHistoryEntry = {
          id: `history-${Date.now()}`,
          projectPath,
          previousContext: originalContext,
          newContext,
          updates,
          appliedAt: new Date().toISOString(),
        };

        update((s) => ({
          ...s,
          isApplying: false,
          pendingUpdate: null,
          showConfirmDialog: false,
          history: [historyEntry, ...s.history].slice(0, 20), // Keep last 20
          lastUpdateAt: historyEntry.appliedAt,
        }));

        return true;
      } catch (error) {
        console.error('[ContextUpdate] Apply failed:', error);
        update((s) => ({
          ...s,
          isApplying: false,
          error: error instanceof Error ? error.message : String(error),
        }));
        return false;
      }
    },

    /**
     * Confirms and applies the pending update
     */
    async confirmPendingUpdate(): Promise<boolean> {
      const state = get({ subscribe });
      if (!state.pendingUpdate) {
        return false;
      }

      const { projectPath, originalContext, updates } = state.pendingUpdate;
      return this.applyUpdates(projectPath, originalContext, updates);
    },

    /**
     * Rejects the pending update
     */
    rejectPendingUpdate() {
      update((s) => ({
        ...s,
        pendingUpdate: null,
        showConfirmDialog: false,
      }));
    },

    /**
     * Undoes the last context update
     */
    async undoLastUpdate(): Promise<boolean> {
      const state = get({ subscribe });
      if (state.history.length === 0 || !contextSaver) {
        return false;
      }

      const lastEntry = state.history[0];

      try {
        // Restore previous context
        await contextSaver(lastEntry.projectPath, lastEntry.previousContext);

        update((s) => ({
          ...s,
          history: s.history.slice(1),
          lastUpdateAt: null,
        }));

        return true;
      } catch (error) {
        console.error('[ContextUpdate] Undo failed:', error);
        update((s) => ({
          ...s,
          error: error instanceof Error ? error.message : String(error),
        }));
        return false;
      }
    },

    /**
     * Clears error state
     */
    clearError() {
      update((s) => ({ ...s, error: null }));
    },

    /**
     * Closes the confirm dialog
     */
    closeConfirmDialog() {
      update((s) => ({ ...s, showConfirmDialog: false }));
    },

    /**
     * Gets notification settings
     */
    shouldShowNotification(): boolean {
      return showNotifications;
    },

    /**
     * Resets the store
     */
    reset() {
      set(initialState);
    },
  };
}

export const contextUpdateStore = createContextUpdateStore();

// Derived stores
export const pendingContextUpdate = derived(
  contextUpdateStore,
  ($s) => $s.pendingUpdate
);

export const showContextUpdateDialog = derived(
  contextUpdateStore,
  ($s) => $s.showConfirmDialog
);

export const isExtractingContextUpdates = derived(
  contextUpdateStore,
  ($s) => $s.isExtracting
);

export const isApplyingContextUpdate = derived(
  contextUpdateStore,
  ($s) => $s.isApplying
);

export const contextUpdateError = derived(
  contextUpdateStore,
  ($s) => $s.error
);

export const canUndoContextUpdate = derived(
  contextUpdateStore,
  ($s) => $s.history.length > 0
);

export const lastContextUpdateTime = derived(
  contextUpdateStore,
  ($s) => $s.lastUpdateAt
);
