// Workflow executor - Processes workflow definitions and creates projects

import type {
  WorkflowDefinition,
  WorkflowAnswers,
  WorkflowExecutionResult,
  WorkflowExecutionProgress,
  TemplateDefinition,
  ContextSectionTemplates,
} from './types.js';

/**
 * File system operations interface (injected from platform layer)
 */
export interface WorkflowFileSystem {
  createDirectory(path: string): Promise<void>;
  writeFile(path: string, content: string): Promise<void>;
  exists(path: string): Promise<boolean>;
  join(...paths: string[]): string;
}

/**
 * LLM call interface for generating content
 */
export type WorkflowLLMCall = (prompt: string) => Promise<string>;

/**
 * Progress callback type
 */
export type ProgressCallback = (progress: WorkflowExecutionProgress) => void;

/**
 * Replaces {{placeholders}} in a template string with values from answers
 */
export function interpolateTemplate(
  template: string,
  answers: WorkflowAnswers
): string {
  return template.replace(/\{\{(\w+)\}\}/g, (match, key) => {
    const value = answers[key];
    if (value === undefined || value === null) {
      return match; // Keep placeholder if no value
    }
    if (Array.isArray(value)) {
      return value.join(', ');
    }
    return String(value);
  });
}

/**
 * Generates the project name from the workflow and answers
 */
export function generateProjectName(
  workflow: WorkflowDefinition,
  answers: WorkflowAnswers
): string {
  if (workflow.projectNameTemplate) {
    return interpolateTemplate(workflow.projectNameTemplate, answers);
  }
  // Default: workflow name + timestamp
  const date = new Date().toISOString().split('T')[0];
  return `${workflow.name} - ${date}`;
}

/**
 * Generates the .project.midlight content
 */
export function generateProjectConfig(
  workflow: WorkflowDefinition,
  projectName: string
): string {
  const config = {
    version: 1,
    name: projectName,
    icon: workflow.icon,
    color: workflow.projectColor || '#6366f1',
    status: 'active',
    createdAt: new Date().toISOString(),
    workflowSource: workflow.id,
    context: {
      includeGlobalContext: true,
      autoUpdateContext: true,
      askBeforeUpdating: false,
    },
  };
  return JSON.stringify(config, null, 2);
}

/**
 * Generates the context.midlight content
 */
export function generateContextDocument(
  contextSections: ContextSectionTemplates,
  answers: WorkflowAnswers
): string {
  const lines: string[] = ['# Project Context', ''];

  // Overview
  lines.push('## Overview');
  lines.push(interpolateTemplate(contextSections.overview, answers));
  lines.push('');

  // Current Status
  lines.push('## Current Status');
  lines.push(
    contextSections.initialStatus
      ? interpolateTemplate(contextSections.initialStatus, answers)
      : 'Just getting started.'
  );
  lines.push('');

  // Key Decisions
  lines.push('## Key Decisions');
  if (contextSections.initialDecisions && contextSections.initialDecisions.length > 0) {
    const today = new Date().toISOString().split('T')[0];
    for (const decision of contextSections.initialDecisions) {
      lines.push(`- [${today}]: ${interpolateTemplate(decision, answers)}`);
    }
  } else {
    lines.push('- [Date]: [Decision description]');
  }
  lines.push('');

  // Open Questions
  lines.push('## Open Questions');
  if (contextSections.initialQuestions && contextSections.initialQuestions.length > 0) {
    for (const question of contextSections.initialQuestions) {
      lines.push(`- [ ] ${interpolateTemplate(question, answers)}`);
    }
  } else {
    lines.push('- [ ] What should I focus on first?');
  }
  lines.push('');

  // AI Notes
  lines.push('## AI Notes');
  lines.push(interpolateTemplate(contextSections.aiNotes, answers));
  lines.push('');

  return lines.join('\n');
}

/**
 * Generates content for a template file
 */
export async function generateFileContent(
  template: TemplateDefinition,
  answers: WorkflowAnswers,
  llmCall?: WorkflowLLMCall
): Promise<string> {
  // If using LLM generation
  if (template.generateWithLLM && llmCall && template.llmPrompt) {
    const prompt = interpolateTemplate(template.llmPrompt, answers);
    try {
      return await llmCall(prompt);
    } catch (error) {
      console.error(`[Workflow] LLM generation failed for ${template.path}:`, error);
      // Fall back to template content
    }
  }

  // Use static template
  if (template.contentTemplate) {
    return interpolateTemplate(template.contentTemplate, answers);
  }

  // Default empty document
  return `# ${template.name}\n\n`;
}

/**
 * Executes a workflow to create a project
 */
export async function executeWorkflow(
  workflow: WorkflowDefinition,
  answers: WorkflowAnswers,
  parentPath: string,
  fs: WorkflowFileSystem,
  options?: {
    llmCall?: WorkflowLLMCall;
    onProgress?: ProgressCallback;
  }
): Promise<WorkflowExecutionResult> {
  const { llmCall, onProgress } = options || {};
  const createdFiles: string[] = [];
  const failedFiles: { path: string; error: string }[] = [];

  const reportProgress = (progress: WorkflowExecutionProgress) => {
    if (onProgress) {
      onProgress(progress);
    }
  };

  try {
    // Generate project name
    const projectName = generateProjectName(workflow, answers);
    const projectPath = fs.join(parentPath, projectName);

    // Phase 1: Create project directory
    reportProgress({
      phase: 'creating-project',
      currentStep: 1,
      totalSteps: 1,
      percentComplete: 10,
    });

    await fs.createDirectory(projectPath);

    // Phase 2: Create .project.midlight
    reportProgress({
      phase: 'creating-project',
      currentStep: 1,
      totalSteps: 1,
      currentFile: '.project.midlight',
      percentComplete: 20,
    });

    const projectConfigPath = fs.join(projectPath, '.project.midlight');
    const projectConfig = generateProjectConfig(workflow, projectName);
    await fs.writeFile(projectConfigPath, projectConfig);
    createdFiles.push('.project.midlight');

    // Phase 3: Create context.midlight
    reportProgress({
      phase: 'creating-context',
      currentStep: 1,
      totalSteps: 1,
      currentFile: 'context.midlight',
      percentComplete: 30,
    });

    const contextPath = fs.join(projectPath, 'context.midlight');
    const contextContent = generateContextDocument(workflow.contextSections, answers);
    await fs.writeFile(contextPath, contextContent);
    createdFiles.push('context.midlight');

    // Phase 4: Create template files
    const fileTemplates = workflow.templates.filter((t) => t.type === 'file');
    const folderTemplates = workflow.templates.filter((t) => t.type === 'folder');
    const totalTemplates = workflow.templates.length;
    let processedTemplates = 0;

    // Create folders first
    for (const folder of folderTemplates) {
      const folderPath = fs.join(projectPath, folder.path);
      try {
        await fs.createDirectory(folderPath);
        createdFiles.push(folder.path);
      } catch (error) {
        failedFiles.push({
          path: folder.path,
          error: error instanceof Error ? error.message : String(error),
        });
      }
      processedTemplates++;
      reportProgress({
        phase: 'creating-files',
        currentStep: processedTemplates,
        totalSteps: totalTemplates,
        currentFile: folder.path,
        percentComplete: 30 + Math.round((processedTemplates / totalTemplates) * 30),
      });
    }

    // Create files
    for (const file of fileTemplates) {
      const filePath = fs.join(projectPath, file.path);
      const isLLMGenerated = file.generateWithLLM && llmCall;

      reportProgress({
        phase: isLLMGenerated ? 'generating-content' : 'creating-files',
        currentStep: processedTemplates + 1,
        totalSteps: totalTemplates,
        currentFile: file.path,
        percentComplete: 60 + Math.round((processedTemplates / totalTemplates) * 35),
      });

      try {
        const content = await generateFileContent(file, answers, llmCall);

        // Wrap content in .midlight JSON format
        const midlightContent = wrapInMidlightFormat(content, file.name);
        await fs.writeFile(filePath, midlightContent);
        createdFiles.push(file.path);
      } catch (error) {
        failedFiles.push({
          path: file.path,
          error: error instanceof Error ? error.message : String(error),
        });
      }
      processedTemplates++;
    }

    // Phase 5: Complete
    reportProgress({
      phase: 'complete',
      currentStep: 1,
      totalSteps: 1,
      percentComplete: 100,
    });

    return {
      success: failedFiles.length === 0,
      projectPath,
      createdFiles,
      failedFiles,
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
      createdFiles,
      failedFiles,
    };
  }
}

/**
 * Wraps markdown content in .midlight JSON format
 */
function wrapInMidlightFormat(markdownContent: string, title: string): string {
  // Convert markdown to simple Tiptap document structure
  const lines = markdownContent.split('\n');
  const content: unknown[] = [];

  for (const line of lines) {
    // Heading detection
    const h1Match = line.match(/^#\s+(.+)$/);
    const h2Match = line.match(/^##\s+(.+)$/);
    const h3Match = line.match(/^###\s+(.+)$/);

    if (h1Match) {
      content.push({
        type: 'heading',
        attrs: { level: 1 },
        content: [{ type: 'text', text: h1Match[1] }],
      });
    } else if (h2Match) {
      content.push({
        type: 'heading',
        attrs: { level: 2 },
        content: [{ type: 'text', text: h2Match[1] }],
      });
    } else if (h3Match) {
      content.push({
        type: 'heading',
        attrs: { level: 3 },
        content: [{ type: 'text', text: h3Match[1] }],
      });
    } else if (line.trim() === '') {
      // Empty paragraph for spacing
      content.push({ type: 'paragraph' });
    } else if (line.startsWith('- ')) {
      // List items (simplified - just as paragraphs for now)
      content.push({
        type: 'paragraph',
        content: [{ type: 'text', text: line }],
      });
    } else {
      content.push({
        type: 'paragraph',
        content: [{ type: 'text', text: line }],
      });
    }
  }

  const doc = {
    version: 1,
    meta: {
      created: new Date().toISOString(),
      modified: new Date().toISOString(),
      title,
    },
    document: {},
    content: {
      type: 'doc',
      content: content.length > 0 ? content : [{ type: 'paragraph' }],
    },
  };

  return JSON.stringify(doc, null, 2);
}

/**
 * Validates interview answers against step requirements
 */
export function validateAnswers(
  workflow: WorkflowDefinition,
  answers: WorkflowAnswers
): { valid: boolean; errors: { stepId: string; message: string }[] } {
  const errors: { stepId: string; message: string }[] = [];

  for (const step of workflow.interview) {
    // Check if step should be shown
    if (step.showIf) {
      const conditionValue = answers[step.showIf.stepId];
      if (step.showIf.equals) {
        const equalsArray = Array.isArray(step.showIf.equals)
          ? step.showIf.equals
          : [step.showIf.equals];
        if (!equalsArray.includes(String(conditionValue))) {
          continue; // Skip validation for hidden steps
        }
      }
      if (step.showIf.notEquals) {
        const notEqualsArray = Array.isArray(step.showIf.notEquals)
          ? step.showIf.notEquals
          : [step.showIf.notEquals];
        if (notEqualsArray.includes(String(conditionValue))) {
          continue; // Skip validation for hidden steps
        }
      }
    }

    const answer = answers[step.id];

    // Required check
    if (step.required) {
      if (answer === undefined || answer === null || answer === '') {
        errors.push({ stepId: step.id, message: 'This field is required' });
        continue;
      }
      if (Array.isArray(answer) && answer.length === 0) {
        errors.push({ stepId: step.id, message: 'Please select at least one option' });
        continue;
      }
    }

    // Validation pattern check
    if (step.validation && answer !== undefined && typeof answer === 'string') {
      const regex = new RegExp(step.validation);
      if (!regex.test(answer)) {
        errors.push({
          stepId: step.id,
          message: step.validationMessage || 'Invalid format',
        });
      }
    }
  }

  return { valid: errors.length === 0, errors };
}
