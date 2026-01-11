// @midlight/stores/workflow - Workflow state management

import { writable, derived, get } from 'svelte/store';
import type {
  WorkflowDefinition,
  WorkflowAnswers,
  WorkflowExecutionResult,
  WorkflowExecutionProgress,
  InterviewStep,
} from '@midlight/core';
import {
  builtInWorkflows,
  getWorkflowById,
  validateAnswers,
  executeWorkflow,
  type WorkflowFileSystem,
  type WorkflowLLMCall,
} from '@midlight/core';

/**
 * Workflow state phases
 */
export type WorkflowPhase = 'idle' | 'selecting' | 'interview' | 'executing' | 'complete' | 'error';

/**
 * Workflow store state
 */
export interface WorkflowState {
  /** Current phase */
  phase: WorkflowPhase;
  /** All available workflows */
  availableWorkflows: WorkflowDefinition[];
  /** Currently selected workflow */
  activeWorkflow: WorkflowDefinition | null;
  /** Current interview step index */
  currentStepIndex: number;
  /** Collected answers */
  answers: WorkflowAnswers;
  /** Validation errors for each step */
  validationErrors: Record<string, string>;
  /** Execution progress */
  executionProgress: WorkflowExecutionProgress | null;
  /** Execution result */
  executionResult: WorkflowExecutionResult | null;
  /** Error message */
  error: string | null;
  /** Parent path where project will be created */
  parentPath: string | null;
}

const initialState: WorkflowState = {
  phase: 'idle',
  availableWorkflows: builtInWorkflows,
  activeWorkflow: null,
  currentStepIndex: 0,
  answers: {},
  validationErrors: {},
  executionProgress: null,
  executionResult: null,
  error: null,
  parentPath: null,
};

// Injected dependencies
let fileSystem: WorkflowFileSystem | null = null;
let llmCall: WorkflowLLMCall | null = null;

function createWorkflowStore() {
  const { subscribe, set, update } = writable<WorkflowState>(initialState);

  return {
    subscribe,

    /**
     * Sets the file system implementation
     */
    setFileSystem(fs: WorkflowFileSystem) {
      fileSystem = fs;
    },

    /**
     * Sets the LLM call implementation
     */
    setLLMCall(call: WorkflowLLMCall) {
      llmCall = call;
    },

    /**
     * Opens the workflow picker
     */
    openPicker(parentPath: string) {
      update((s) => ({
        ...s,
        phase: 'selecting',
        parentPath,
        activeWorkflow: null,
        currentStepIndex: 0,
        answers: {},
        validationErrors: {},
        executionProgress: null,
        executionResult: null,
        error: null,
      }));
    },

    /**
     * Selects a workflow and starts the interview
     */
    selectWorkflow(workflowId: string) {
      const workflow = getWorkflowById(workflowId);
      if (!workflow) {
        update((s) => ({
          ...s,
          error: `Workflow not found: ${workflowId}`,
        }));
        return;
      }

      update((s) => ({
        ...s,
        phase: 'interview',
        activeWorkflow: workflow,
        currentStepIndex: 0,
        answers: {},
        validationErrors: {},
      }));
    },

    /**
     * Sets an answer for the current step
     */
    setAnswer(stepId: string, value: string | number | string[]) {
      update((s) => ({
        ...s,
        answers: { ...s.answers, [stepId]: value },
        validationErrors: { ...s.validationErrors, [stepId]: '' },
      }));
    },

    /**
     * Moves to the next interview step
     */
    nextStep() {
      const state = get({ subscribe });
      if (!state.activeWorkflow) return;

      const currentStep = state.activeWorkflow.interview[state.currentStepIndex];

      // Validate current step
      if (currentStep.required) {
        const answer = state.answers[currentStep.id];
        if (answer === undefined || answer === '' || (Array.isArray(answer) && answer.length === 0)) {
          update((s) => ({
            ...s,
            validationErrors: {
              ...s.validationErrors,
              [currentStep.id]: 'This field is required',
            },
          }));
          return;
        }
      }

      // Find next visible step
      let nextIndex = state.currentStepIndex + 1;
      while (nextIndex < state.activeWorkflow.interview.length) {
        const nextStep = state.activeWorkflow.interview[nextIndex];
        if (this.isStepVisible(nextStep, state.answers)) {
          break;
        }
        nextIndex++;
      }

      if (nextIndex >= state.activeWorkflow.interview.length) {
        // All steps complete, validate all and start execution
        const validation = validateAnswers(state.activeWorkflow, state.answers);
        if (!validation.valid) {
          const errors: Record<string, string> = {};
          for (const error of validation.errors) {
            errors[error.stepId] = error.message;
          }
          update((s) => ({
            ...s,
            validationErrors: errors,
          }));
          return;
        }

        this.startExecution();
      } else {
        update((s) => ({
          ...s,
          currentStepIndex: nextIndex,
        }));
      }
    },

    /**
     * Moves to the previous interview step
     */
    previousStep() {
      const state = get({ subscribe });
      if (!state.activeWorkflow) return;

      // Find previous visible step
      let prevIndex = state.currentStepIndex - 1;
      while (prevIndex >= 0) {
        const prevStep = state.activeWorkflow.interview[prevIndex];
        if (this.isStepVisible(prevStep, state.answers)) {
          break;
        }
        prevIndex--;
      }

      if (prevIndex < 0) {
        // Go back to workflow selection
        update((s) => ({
          ...s,
          phase: 'selecting',
          activeWorkflow: null,
        }));
      } else {
        update((s) => ({
          ...s,
          currentStepIndex: prevIndex,
        }));
      }
    },

    /**
     * Checks if a step should be visible based on conditions
     */
    isStepVisible(step: InterviewStep, answers: WorkflowAnswers): boolean {
      if (!step.showIf) return true;

      const conditionValue = answers[step.showIf.stepId];

      if (step.showIf.equals) {
        const equalsArray = Array.isArray(step.showIf.equals)
          ? step.showIf.equals
          : [step.showIf.equals];
        return equalsArray.includes(String(conditionValue));
      }

      if (step.showIf.notEquals) {
        const notEqualsArray = Array.isArray(step.showIf.notEquals)
          ? step.showIf.notEquals
          : [step.showIf.notEquals];
        return !notEqualsArray.includes(String(conditionValue));
      }

      return true;
    },

    /**
     * Starts workflow execution
     */
    async startExecution() {
      const state = get({ subscribe });
      if (!state.activeWorkflow || !state.parentPath || !fileSystem) {
        update((s) => ({
          ...s,
          phase: 'error',
          error: 'Missing workflow, parent path, or file system',
        }));
        return;
      }

      update((s) => ({
        ...s,
        phase: 'executing',
        executionProgress: {
          phase: 'creating-project',
          currentStep: 0,
          totalSteps: 1,
          percentComplete: 0,
        },
      }));

      try {
        const result = await executeWorkflow(
          state.activeWorkflow,
          state.answers,
          state.parentPath,
          fileSystem,
          {
            llmCall: llmCall || undefined,
            onProgress: (progress) => {
              update((s) => ({
                ...s,
                executionProgress: progress,
              }));
            },
          }
        );

        update((s) => ({
          ...s,
          phase: result.success ? 'complete' : 'error',
          executionResult: result,
          error: result.error || null,
        }));
      } catch (error) {
        update((s) => ({
          ...s,
          phase: 'error',
          error: error instanceof Error ? error.message : String(error),
        }));
      }
    },

    /**
     * Cancels the current workflow
     */
    cancel() {
      set(initialState);
    },

    /**
     * Resets and closes the workflow
     */
    close() {
      set(initialState);
    },

    /**
     * Gets the total number of visible steps
     */
    getVisibleStepCount(): number {
      const state = get({ subscribe });
      if (!state.activeWorkflow) return 0;

      return state.activeWorkflow.interview.filter((step) =>
        this.isStepVisible(step, state.answers)
      ).length;
    },

    /**
     * Gets the current visible step index (1-based)
     */
    getCurrentVisibleStepNumber(): number {
      const state = get({ subscribe });
      if (!state.activeWorkflow) return 0;

      let visibleCount = 0;
      for (let i = 0; i <= state.currentStepIndex; i++) {
        const step = state.activeWorkflow.interview[i];
        if (this.isStepVisible(step, state.answers)) {
          visibleCount++;
        }
      }
      return visibleCount;
    },
  };
}

export const workflowStore = createWorkflowStore();

// Derived stores
export const workflowPhase = derived(workflowStore, ($ws) => $ws.phase);

export const activeWorkflow = derived(workflowStore, ($ws) => $ws.activeWorkflow);

export const currentStep = derived(workflowStore, ($ws) => {
  if (!$ws.activeWorkflow) return null;
  return $ws.activeWorkflow.interview[$ws.currentStepIndex];
});

export const workflowAnswers = derived(workflowStore, ($ws) => $ws.answers);

export const validationErrors = derived(workflowStore, ($ws) => $ws.validationErrors);

export const executionProgress = derived(workflowStore, ($ws) => $ws.executionProgress);

export const executionResult = derived(workflowStore, ($ws) => $ws.executionResult);

export const workflowError = derived(workflowStore, ($ws) => $ws.error);

export const isWorkflowActive = derived(
  workflowStore,
  ($ws) => $ws.phase !== 'idle'
);

export const availableWorkflows = derived(
  workflowStore,
  ($ws) => $ws.availableWorkflows
);

export const isFirstStep = derived(workflowStore, ($ws) => {
  if (!$ws.activeWorkflow) return true;
  // Check if there are any visible steps before the current one
  for (let i = 0; i < $ws.currentStepIndex; i++) {
    const step = $ws.activeWorkflow.interview[i];
    if (workflowStore.isStepVisible(step, $ws.answers)) {
      return false;
    }
  }
  return true;
});

export const isLastStep = derived(workflowStore, ($ws) => {
  if (!$ws.activeWorkflow) return true;
  // Check if there are any visible steps after the current one
  for (let i = $ws.currentStepIndex + 1; i < $ws.activeWorkflow.interview.length; i++) {
    const step = $ws.activeWorkflow.interview[i];
    if (workflowStore.isStepVisible(step, $ws.answers)) {
      return false;
    }
  }
  return true;
});
