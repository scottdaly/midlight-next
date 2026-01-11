// Workflows module - Interview-based project scaffolding

export type {
  InterviewStepType,
  InterviewStep,
  TemplateDefinition,
  ContextSectionTemplates,
  WorkflowDefinition,
  WorkflowAnswers,
  WorkflowExecutionResult,
  WorkflowExecutionProgress,
  WorkflowCategory,
} from './types.js';

export { WORKFLOW_CATEGORIES } from './types.js';

export type { WorkflowFileSystem, WorkflowLLMCall, ProgressCallback } from './executor.js';

export {
  interpolateTemplate,
  generateProjectName,
  generateProjectConfig,
  generateContextDocument,
  generateFileContent,
  executeWorkflow,
  validateAnswers,
} from './executor.js';

export {
  weightLossWorkflow,
  bigPurchaseWorkflow,
  bookWritingWorkflow,
  builtInWorkflows,
  getWorkflowById,
  getWorkflowsByCategory,
} from './definitions.js';
