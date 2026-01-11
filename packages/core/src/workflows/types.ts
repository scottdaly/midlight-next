// Workflow types - Schema for interview-based project scaffolding

/**
 * Type of input for an interview step
 */
export type InterviewStepType = 'text' | 'number' | 'select' | 'multiselect' | 'date' | 'textarea';

/**
 * A single step in the workflow interview
 */
export interface InterviewStep {
  /** Unique identifier for this step */
  id: string;
  /** The question to ask the user */
  question: string;
  /** Type of input expected */
  type: InterviewStepType;
  /** Options for select/multiselect types */
  options?: string[];
  /** Whether an answer is required */
  required: boolean;
  /** Placeholder text for input */
  placeholder?: string;
  /** Help text shown below the input */
  helpText?: string;
  /** Default value */
  defaultValue?: string | number | string[];
  /** Validation regex pattern (for text inputs) */
  validation?: string;
  /** Error message if validation fails */
  validationMessage?: string;
  /** Condition to show this step (based on previous answers) */
  showIf?: {
    stepId: string;
    equals?: string | string[];
    notEquals?: string | string[];
  };
}

/**
 * A template for a file or folder to create
 */
export interface TemplateDefinition {
  /** Relative path within the project */
  path: string;
  /** Display name for UI */
  name: string;
  /** Whether this is a file or folder */
  type: 'file' | 'folder';
  /** Static markdown content with {{placeholders}} */
  contentTemplate?: string;
  /** Use LLM to generate content (uses interview answers as context) */
  generateWithLLM?: boolean;
  /** Prompt for LLM generation (if generateWithLLM is true) */
  llmPrompt?: string;
  /** Whether to show this file after creation */
  openAfterCreate?: boolean;
}

/**
 * Templates for context.midlight sections
 */
export interface ContextSectionTemplates {
  /** Template for the Overview section */
  overview: string;
  /** Template for the AI Notes section */
  aiNotes: string;
  /** Template for initial status */
  initialStatus?: string;
  /** Initial key decisions */
  initialDecisions?: string[];
  /** Initial open questions */
  initialQuestions?: string[];
}

/**
 * A complete workflow definition
 */
export interface WorkflowDefinition {
  /** Unique identifier */
  id: string;
  /** Display name */
  name: string;
  /** Short description */
  description: string;
  /** Icon identifier (emoji or icon name) */
  icon: string;
  /** Category for grouping */
  category: string;
  /** Interview steps */
  interview: InterviewStep[];
  /** Files and folders to create */
  templates: TemplateDefinition[];
  /** Context document configuration */
  contextSections: ContextSectionTemplates;
  /** Suggested project name template with {{placeholders}} */
  projectNameTemplate?: string;
  /** Suggested project color */
  projectColor?: string;
}

/**
 * Answers collected during the interview
 */
export type WorkflowAnswers = Record<string, string | number | string[] | undefined>;

/**
 * Result of executing a workflow
 */
export interface WorkflowExecutionResult {
  /** Whether execution succeeded */
  success: boolean;
  /** Path to the created project */
  projectPath?: string;
  /** Error message if failed */
  error?: string;
  /** Files that were created */
  createdFiles: string[];
  /** Files that failed to create */
  failedFiles: { path: string; error: string }[];
}

/**
 * Progress during workflow execution
 */
export interface WorkflowExecutionProgress {
  /** Current phase */
  phase: 'creating-project' | 'creating-context' | 'creating-files' | 'generating-content' | 'complete';
  /** Current step within the phase */
  currentStep: number;
  /** Total steps in the phase */
  totalSteps: number;
  /** Current file being processed */
  currentFile?: string;
  /** Percentage complete (0-100) */
  percentComplete: number;
}

/**
 * Category for grouping workflows
 */
export interface WorkflowCategory {
  id: string;
  name: string;
  description: string;
  icon: string;
}

/**
 * Built-in workflow categories
 */
export const WORKFLOW_CATEGORIES: WorkflowCategory[] = [
  { id: 'health', name: 'Health & Wellness', description: 'Track health goals and habits', icon: 'heart' },
  { id: 'finance', name: 'Finance', description: 'Manage money decisions and budgets', icon: 'wallet' },
  { id: 'creative', name: 'Creative', description: 'Writing, art, and creative projects', icon: 'pen' },
  { id: 'learning', name: 'Learning', description: 'Education and skill development', icon: 'book' },
  { id: 'productivity', name: 'Productivity', description: 'Organization and task management', icon: 'checklist' },
  { id: 'other', name: 'Other', description: 'General purpose projects', icon: 'folder' },
];
