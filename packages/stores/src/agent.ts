// @midlight/stores/agent - Agent execution state management

import { writable, derived, get } from 'svelte/store';
import { generateId } from '@midlight/core/utils';
import type { ToolCall } from '@midlight/core';

// ============================================================================
// Types
// ============================================================================

export type ToolExecutionStatus = 'pending' | 'running' | 'completed' | 'failed' | 'requires_confirmation';

export interface ToolExecution {
  id: string;
  toolName: string;
  arguments: Record<string, unknown>;
  status: ToolExecutionStatus;
  result?: ToolResult;
  startedAt: string;
  completedAt?: string;
  description?: string;
}

export interface ToolResult {
  success: boolean;
  data?: unknown;
  error?: string;
}

export interface PendingChange {
  changeId: string;
  path: string;
  originalContent: string;
  newContent: string;
  description?: string;
  createdAt: string;
  toolExecutionId: string;
}

export interface AgentState {
  isRunning: boolean;
  currentExecutionId: string | null;
  executions: ToolExecution[];
  pendingChanges: PendingChange[];
  error: string | null;
}

// ============================================================================
// Initial State
// ============================================================================

const initialState: AgentState = {
  isRunning: false,
  currentExecutionId: null,
  executions: [],
  pendingChanges: [],
  error: null,
};

// ============================================================================
// Store
// ============================================================================

function createAgentStore() {
  const { subscribe, set, update } = writable<AgentState>(initialState);

  return {
    subscribe,

    /**
     * Starts a new tool execution
     */
    startExecution(toolCall: ToolCall, description?: string): string {
      const id = generateId();
      const now = new Date().toISOString();

      update((s) => ({
        ...s,
        isRunning: true,
        currentExecutionId: id,
        executions: [
          ...s.executions,
          {
            id,
            toolName: toolCall.name,
            arguments: toolCall.arguments,
            status: 'running',
            startedAt: now,
            description,
          },
        ],
        error: null,
      }));

      return id;
    },

    /**
     * Completes a tool execution with result
     */
    completeExecution(executionId: string, result: ToolResult) {
      const now = new Date().toISOString();

      update((s) => ({
        ...s,
        isRunning: s.currentExecutionId === executionId ? false : s.isRunning,
        currentExecutionId: s.currentExecutionId === executionId ? null : s.currentExecutionId,
        executions: s.executions.map((e) =>
          e.id === executionId
            ? {
                ...e,
                status: result.success ? 'completed' : 'failed',
                result,
                completedAt: now,
              }
            : e
        ),
      }));
    },

    /**
     * Marks an execution as requiring confirmation
     */
    requireConfirmation(executionId: string) {
      update((s) => ({
        ...s,
        executions: s.executions.map((e) =>
          e.id === executionId
            ? { ...e, status: 'requires_confirmation' }
            : e
        ),
      }));
    },

    /**
     * Adds a pending change from an edit operation
     */
    addPendingChange(change: Omit<PendingChange, 'createdAt'>) {
      const now = new Date().toISOString();

      update((s) => ({
        ...s,
        pendingChanges: [
          ...s.pendingChanges,
          { ...change, createdAt: now },
        ],
      }));
    },

    /**
     * Accepts a pending change (keeps the new content)
     */
    acceptChange(changeId: string) {
      update((s) => ({
        ...s,
        pendingChanges: s.pendingChanges.filter((c) => c.changeId !== changeId),
      }));
    },

    /**
     * Rejects a pending change (reverts to original)
     * Note: The actual revert should be handled by the caller
     */
    rejectChange(changeId: string) {
      update((s) => ({
        ...s,
        pendingChanges: s.pendingChanges.filter((c) => c.changeId !== changeId),
      }));
    },

    /**
     * Gets a pending change by ID
     */
    getPendingChange(changeId: string): PendingChange | undefined {
      return get({ subscribe }).pendingChanges.find((c) => c.changeId === changeId);
    },

    /**
     * Sets error state
     */
    setError(error: string | null) {
      update((s) => ({ ...s, error, isRunning: false }));
    },

    /**
     * Clears all executions and pending changes
     */
    clear() {
      update((s) => ({
        ...s,
        executions: [],
        pendingChanges: [],
        error: null,
      }));
    },

    /**
     * Resets the store to initial state
     */
    reset() {
      set(initialState);
    },
  };
}

export const agent = createAgentStore();

// ============================================================================
// Derived Stores
// ============================================================================

export const isAgentRunning = derived(agent, ($agent) => $agent.isRunning);

export const currentExecution = derived(agent, ($agent) =>
  $agent.executions.find((e) => e.id === $agent.currentExecutionId)
);

export const pendingChanges = derived(agent, ($agent) => $agent.pendingChanges);

export const hasPendingChanges = derived(
  agent,
  ($agent) => $agent.pendingChanges.length > 0
);

export const recentExecutions = derived(agent, ($agent) =>
  $agent.executions.slice(-10).reverse()
);
