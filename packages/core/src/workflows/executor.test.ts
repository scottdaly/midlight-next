// Tests for workflow executor

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  interpolateTemplate,
  generateProjectName,
  generateProjectConfig,
  generateContextDocument,
  generateFileContent,
  executeWorkflow,
  validateAnswers,
  type WorkflowFileSystem,
  type WorkflowLLMCall,
} from './executor.js';
import type {
  WorkflowDefinition,
  WorkflowAnswers,
  ContextSectionTemplates,
  TemplateDefinition,
  WorkflowExecutionProgress,
} from './types.js';

describe('interpolateTemplate', () => {
  it('replaces single placeholder with string value', () => {
    const template = 'Hello, {{name}}!';
    const answers: WorkflowAnswers = { name: 'World' };
    expect(interpolateTemplate(template, answers)).toBe('Hello, World!');
  });

  it('replaces multiple placeholders', () => {
    const template = '{{greeting}}, {{name}}!';
    const answers: WorkflowAnswers = { greeting: 'Hello', name: 'World' };
    expect(interpolateTemplate(template, answers)).toBe('Hello, World!');
  });

  it('replaces same placeholder multiple times', () => {
    const template = '{{name}} loves {{name}}';
    const answers: WorkflowAnswers = { name: 'Alice' };
    expect(interpolateTemplate(template, answers)).toBe('Alice loves Alice');
  });

  it('joins array values with commas', () => {
    const template = 'Selected: {{options}}';
    const answers: WorkflowAnswers = { options: ['A', 'B', 'C'] };
    expect(interpolateTemplate(template, answers)).toBe('Selected: A, B, C');
  });

  it('converts number values to strings', () => {
    const template = 'Age: {{age}} years';
    const answers: WorkflowAnswers = { age: 25 };
    expect(interpolateTemplate(template, answers)).toBe('Age: 25 years');
  });

  it('keeps placeholder if value is undefined', () => {
    const template = 'Hello, {{name}}!';
    const answers: WorkflowAnswers = {};
    expect(interpolateTemplate(template, answers)).toBe('Hello, {{name}}!');
  });

  it('keeps placeholder if value is null', () => {
    const template = 'Hello, {{name}}!';
    const answers: WorkflowAnswers = { name: null as unknown as string };
    expect(interpolateTemplate(template, answers)).toBe('Hello, {{name}}!');
  });

  it('handles empty string value', () => {
    const template = 'Hello, {{name}}!';
    const answers: WorkflowAnswers = { name: '' };
    expect(interpolateTemplate(template, answers)).toBe('Hello, !');
  });

  it('handles template with no placeholders', () => {
    const template = 'No placeholders here';
    const answers: WorkflowAnswers = { name: 'ignored' };
    expect(interpolateTemplate(template, answers)).toBe('No placeholders here');
  });

  it('handles complex template with mixed content', () => {
    const template = `# {{title}}

Goal: {{goal}}
Approaches: {{approaches}}

Notes: {{notes}}`;
    const answers: WorkflowAnswers = {
      title: 'My Project',
      goal: 'Lose weight',
      approaches: ['Diet', 'Exercise'],
      notes: undefined,
    };
    expect(interpolateTemplate(template, answers)).toBe(`# My Project

Goal: Lose weight
Approaches: Diet, Exercise

Notes: {{notes}}`);
  });
});

describe('generateProjectName', () => {
  const mockWorkflow: WorkflowDefinition = {
    id: 'test',
    name: 'Test Workflow',
    description: 'A test workflow',
    icon: '🧪',
    category: 'test',
    interview: [],
    templates: [],
    contextSections: {
      overview: '',
      aiNotes: '',
    },
  };

  it('uses projectNameTemplate when provided', () => {
    const workflow = { ...mockWorkflow, projectNameTemplate: '{{title}} - {{type}}' };
    const answers: WorkflowAnswers = { title: 'My Project', type: 'Research' };
    expect(generateProjectName(workflow, answers)).toBe('My Project - Research');
  });

  it('falls back to workflow name with date when no template', () => {
    const name = generateProjectName(mockWorkflow, {});
    // Check it starts with workflow name
    expect(name).toMatch(/^Test Workflow - \d{4}-\d{2}-\d{2}$/);
  });

  it('handles missing placeholder values in template', () => {
    const workflow = { ...mockWorkflow, projectNameTemplate: '{{title}} Project' };
    const answers: WorkflowAnswers = {};
    expect(generateProjectName(workflow, answers)).toBe('{{title}} Project');
  });
});

describe('generateProjectConfig', () => {
  const mockWorkflow: WorkflowDefinition = {
    id: 'weight-loss',
    name: 'Weight Loss',
    description: 'Track weight loss',
    icon: '⚖️',
    category: 'health',
    projectColor: '#10b981',
    interview: [],
    templates: [],
    contextSections: {
      overview: '',
      aiNotes: '',
    },
  };

  it('generates valid JSON config', () => {
    const config = generateProjectConfig(mockWorkflow, 'My Weight Loss Journey');
    const parsed = JSON.parse(config);

    expect(parsed.version).toBe(1);
    expect(parsed.name).toBe('My Weight Loss Journey');
    expect(parsed.icon).toBe('⚖️');
    expect(parsed.color).toBe('#10b981');
    expect(parsed.status).toBe('active');
    expect(parsed.workflowSource).toBe('weight-loss');
    expect(parsed.context.includeGlobalContext).toBe(true);
    expect(parsed.context.autoUpdateContext).toBe(true);
    expect(parsed.context.askBeforeUpdating).toBe(false);
  });

  it('uses default color when not specified', () => {
    const workflow = { ...mockWorkflow, projectColor: undefined };
    const config = generateProjectConfig(workflow, 'Test');
    const parsed = JSON.parse(config);
    expect(parsed.color).toBe('#6366f1');
  });

  it('includes valid createdAt timestamp', () => {
    const config = generateProjectConfig(mockWorkflow, 'Test');
    const parsed = JSON.parse(config);
    expect(() => new Date(parsed.createdAt)).not.toThrow();
  });
});

describe('generateContextDocument', () => {
  it('generates context with all sections', () => {
    const contextSections: ContextSectionTemplates = {
      overview: 'Weight loss from {{current}} to {{goal}}',
      aiNotes: 'Help track progress for {{name}}',
      initialStatus: 'Starting the journey',
      initialDecisions: ['Goal set: {{goal}}', 'Timeline: {{timeline}}'],
      initialQuestions: ['Best exercises?', 'How to meal prep?'],
    };
    const answers: WorkflowAnswers = {
      current: '180 lbs',
      goal: '160 lbs',
      timeline: '3 months',
      name: 'John',
    };

    const doc = generateContextDocument(contextSections, answers);

    expect(doc).toContain('# Project Context');
    expect(doc).toContain('## Overview');
    expect(doc).toContain('Weight loss from 180 lbs to 160 lbs');
    expect(doc).toContain('## Current Status');
    expect(doc).toContain('Starting the journey');
    expect(doc).toContain('## Key Decisions');
    expect(doc).toContain('Goal set: 160 lbs');
    expect(doc).toContain('Timeline: 3 months');
    expect(doc).toContain('## Open Questions');
    expect(doc).toContain('- [ ] Best exercises?');
    expect(doc).toContain('- [ ] How to meal prep?');
    expect(doc).toContain('## AI Notes');
    expect(doc).toContain('Help track progress for John');
  });

  it('uses default status when not provided', () => {
    const contextSections: ContextSectionTemplates = {
      overview: 'Overview',
      aiNotes: 'Notes',
    };
    const doc = generateContextDocument(contextSections, {});
    expect(doc).toContain('Just getting started.');
  });

  it('uses default questions when not provided', () => {
    const contextSections: ContextSectionTemplates = {
      overview: 'Overview',
      aiNotes: 'Notes',
    };
    const doc = generateContextDocument(contextSections, {});
    expect(doc).toContain('- [ ] What should I focus on first?');
  });

  it('uses default decision format when not provided', () => {
    const contextSections: ContextSectionTemplates = {
      overview: 'Overview',
      aiNotes: 'Notes',
    };
    const doc = generateContextDocument(contextSections, {});
    expect(doc).toContain('- [Date]: [Decision description]');
  });

  it('includes date prefix for decisions', () => {
    const contextSections: ContextSectionTemplates = {
      overview: 'Overview',
      aiNotes: 'Notes',
      initialDecisions: ['Made a choice'],
    };
    const doc = generateContextDocument(contextSections, {});
    // Should have today's date in format YYYY-MM-DD
    expect(doc).toMatch(/- \[\d{4}-\d{2}-\d{2}\]: Made a choice/);
  });
});

describe('generateFileContent', () => {
  it('uses static template when available', async () => {
    const template: TemplateDefinition = {
      path: 'test.midlight',
      name: 'Test',
      type: 'file',
      contentTemplate: '# Hello {{name}}',
    };
    const answers: WorkflowAnswers = { name: 'World' };

    const content = await generateFileContent(template, answers);
    expect(content).toBe('# Hello World');
  });

  it('generates default content when no template', async () => {
    const template: TemplateDefinition = {
      path: 'test.midlight',
      name: 'My Document',
      type: 'file',
    };

    const content = await generateFileContent(template, {});
    expect(content).toBe('# My Document\n\n');
  });

  it('uses LLM when generateWithLLM is true', async () => {
    const template: TemplateDefinition = {
      path: 'test.midlight',
      name: 'Test',
      type: 'file',
      generateWithLLM: true,
      llmPrompt: 'Write about {{topic}}',
    };
    const answers: WorkflowAnswers = { topic: 'testing' };
    const mockLLM: WorkflowLLMCall = vi.fn().mockResolvedValue('Generated content about testing');

    const content = await generateFileContent(template, answers, mockLLM);

    expect(mockLLM).toHaveBeenCalledWith('Write about testing');
    expect(content).toBe('Generated content about testing');
  });

  it('falls back to template when LLM fails', async () => {
    const template: TemplateDefinition = {
      path: 'test.midlight',
      name: 'Test',
      type: 'file',
      generateWithLLM: true,
      llmPrompt: 'Generate content',
      contentTemplate: 'Fallback content',
    };
    const mockLLM: WorkflowLLMCall = vi.fn().mockRejectedValue(new Error('API error'));

    const content = await generateFileContent(template, {}, mockLLM);
    expect(content).toBe('Fallback content');
  });

  it('falls back to default when LLM fails and no template', async () => {
    const template: TemplateDefinition = {
      path: 'test.midlight',
      name: 'Test Document',
      type: 'file',
      generateWithLLM: true,
      llmPrompt: 'Generate content',
    };
    const mockLLM: WorkflowLLMCall = vi.fn().mockRejectedValue(new Error('API error'));

    const content = await generateFileContent(template, {}, mockLLM);
    expect(content).toBe('# Test Document\n\n');
  });

  it('does not call LLM when llmCall is not provided', async () => {
    const template: TemplateDefinition = {
      path: 'test.midlight',
      name: 'Test',
      type: 'file',
      generateWithLLM: true,
      llmPrompt: 'Generate content',
      contentTemplate: 'Static content',
    };

    const content = await generateFileContent(template, {});
    expect(content).toBe('Static content');
  });
});

describe('validateAnswers', () => {
  const createWorkflow = (interview: WorkflowDefinition['interview']): WorkflowDefinition => ({
    id: 'test',
    name: 'Test',
    description: 'Test',
    icon: '🧪',
    category: 'test',
    interview,
    templates: [],
    contextSections: { overview: '', aiNotes: '' },
  });

  it('passes when all required fields are filled', () => {
    const workflow = createWorkflow([
      { id: 'name', question: 'Name?', type: 'text', required: true },
      { id: 'age', question: 'Age?', type: 'number', required: true },
    ]);
    const answers: WorkflowAnswers = { name: 'John', age: 25 };

    const result = validateAnswers(workflow, answers);
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it('fails when required text field is missing', () => {
    const workflow = createWorkflow([
      { id: 'name', question: 'Name?', type: 'text', required: true },
    ]);

    const result = validateAnswers(workflow, {});
    expect(result.valid).toBe(false);
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0]).toEqual({
      stepId: 'name',
      message: 'This field is required',
    });
  });

  it('fails when required field is empty string', () => {
    const workflow = createWorkflow([
      { id: 'name', question: 'Name?', type: 'text', required: true },
    ]);
    const answers: WorkflowAnswers = { name: '' };

    const result = validateAnswers(workflow, answers);
    expect(result.valid).toBe(false);
    expect(result.errors[0].message).toBe('This field is required');
  });

  it('fails when required multiselect has no selections', () => {
    const workflow = createWorkflow([
      { id: 'choices', question: 'Choices?', type: 'multiselect', options: ['A', 'B'], required: true },
    ]);
    const answers: WorkflowAnswers = { choices: [] };

    const result = validateAnswers(workflow, answers);
    expect(result.valid).toBe(false);
    expect(result.errors[0].message).toBe('Please select at least one option');
  });

  it('passes with optional field empty', () => {
    const workflow = createWorkflow([
      { id: 'notes', question: 'Notes?', type: 'textarea', required: false },
    ]);

    const result = validateAnswers(workflow, {});
    expect(result.valid).toBe(true);
  });

  it('validates regex pattern', () => {
    const workflow = createWorkflow([
      {
        id: 'email',
        question: 'Email?',
        type: 'text',
        required: true,
        validation: '^[^@]+@[^@]+\\.[^@]+$',
        validationMessage: 'Please enter a valid email',
      },
    ]);
    const answers: WorkflowAnswers = { email: 'invalid-email' };

    const result = validateAnswers(workflow, answers);
    expect(result.valid).toBe(false);
    expect(result.errors[0].message).toBe('Please enter a valid email');
  });

  it('passes valid regex pattern', () => {
    const workflow = createWorkflow([
      {
        id: 'email',
        question: 'Email?',
        type: 'text',
        required: true,
        validation: '^[^@]+@[^@]+\\.[^@]+$',
      },
    ]);
    const answers: WorkflowAnswers = { email: 'test@example.com' };

    const result = validateAnswers(workflow, answers);
    expect(result.valid).toBe(true);
  });

  it('uses default validation message when not provided', () => {
    const workflow = createWorkflow([
      {
        id: 'code',
        question: 'Code?',
        type: 'text',
        required: true,
        validation: '^[A-Z]{3}$',
      },
    ]);
    const answers: WorkflowAnswers = { code: 'abc' };

    const result = validateAnswers(workflow, answers);
    expect(result.errors[0].message).toBe('Invalid format');
  });

  describe('conditional validation with showIf', () => {
    it('skips validation when showIf.equals condition not met', () => {
      const workflow = createWorkflow([
        { id: 'has_pet', question: 'Pet?', type: 'select', options: ['Yes', 'No'], required: true },
        {
          id: 'pet_name',
          question: 'Pet name?',
          type: 'text',
          required: true,
          showIf: { stepId: 'has_pet', equals: 'Yes' },
        },
      ]);
      const answers: WorkflowAnswers = { has_pet: 'No' };

      const result = validateAnswers(workflow, answers);
      expect(result.valid).toBe(true);
    });

    it('validates when showIf.equals condition is met', () => {
      const workflow = createWorkflow([
        { id: 'has_pet', question: 'Pet?', type: 'select', options: ['Yes', 'No'], required: true },
        {
          id: 'pet_name',
          question: 'Pet name?',
          type: 'text',
          required: true,
          showIf: { stepId: 'has_pet', equals: 'Yes' },
        },
      ]);
      const answers: WorkflowAnswers = { has_pet: 'Yes' };

      const result = validateAnswers(workflow, answers);
      expect(result.valid).toBe(false);
      expect(result.errors[0].stepId).toBe('pet_name');
    });

    it('handles showIf.equals with array of values', () => {
      const workflow = createWorkflow([
        { id: 'status', question: 'Status?', type: 'select', options: ['A', 'B', 'C'], required: true },
        {
          id: 'details',
          question: 'Details?',
          type: 'text',
          required: true,
          showIf: { stepId: 'status', equals: ['A', 'B'] },
        },
      ]);

      // Status A - should validate
      expect(validateAnswers(workflow, { status: 'A' }).valid).toBe(false);
      // Status B - should validate
      expect(validateAnswers(workflow, { status: 'B' }).valid).toBe(false);
      // Status C - should skip validation
      expect(validateAnswers(workflow, { status: 'C' }).valid).toBe(true);
    });

    it('skips validation when showIf.notEquals condition is met', () => {
      const workflow = createWorkflow([
        { id: 'type', question: 'Type?', type: 'select', options: ['Basic', 'Advanced'], required: true },
        {
          id: 'advanced_options',
          question: 'Options?',
          type: 'text',
          required: true,
          showIf: { stepId: 'type', notEquals: 'Basic' },
        },
      ]);
      const answers: WorkflowAnswers = { type: 'Basic' };

      const result = validateAnswers(workflow, answers);
      expect(result.valid).toBe(true);
    });

    it('validates when showIf.notEquals condition is not met', () => {
      const workflow = createWorkflow([
        { id: 'type', question: 'Type?', type: 'select', options: ['Basic', 'Advanced'], required: true },
        {
          id: 'advanced_options',
          question: 'Options?',
          type: 'text',
          required: true,
          showIf: { stepId: 'type', notEquals: 'Basic' },
        },
      ]);
      const answers: WorkflowAnswers = { type: 'Advanced' };

      const result = validateAnswers(workflow, answers);
      expect(result.valid).toBe(false);
      expect(result.errors[0].stepId).toBe('advanced_options');
    });
  });
});

describe('executeWorkflow', () => {
  let mockFs: WorkflowFileSystem;
  let mockLLM: WorkflowLLMCall;
  let progressCalls: WorkflowExecutionProgress[];

  beforeEach(() => {
    mockFs = {
      createDirectory: vi.fn().mockResolvedValue(undefined),
      writeFile: vi.fn().mockResolvedValue(undefined),
      exists: vi.fn().mockResolvedValue(false),
      join: (...paths: string[]) => paths.join('/').replace(/\/+/g, '/'),
    };
    mockLLM = vi.fn().mockResolvedValue('Generated content');
    progressCalls = [];
  });

  const createMinimalWorkflow = (): WorkflowDefinition => ({
    id: 'test',
    name: 'Test Workflow',
    description: 'A test',
    icon: '🧪',
    category: 'test',
    projectNameTemplate: '{{name}}',
    interview: [],
    templates: [],
    contextSections: {
      overview: 'Test project',
      aiNotes: 'Help with testing',
    },
  });

  it('creates project directory', async () => {
    const workflow = createMinimalWorkflow();
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs);

    expect(mockFs.createDirectory).toHaveBeenCalledWith('/parent/MyProject');
  });

  it('creates .project.midlight file', async () => {
    const workflow = createMinimalWorkflow();
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs);

    expect(mockFs.writeFile).toHaveBeenCalledWith(
      '/parent/MyProject/.project.midlight',
      expect.stringContaining('"name": "MyProject"')
    );
  });

  it('creates context.midlight file', async () => {
    const workflow = createMinimalWorkflow();
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs);

    expect(mockFs.writeFile).toHaveBeenCalledWith(
      '/parent/MyProject/context.midlight',
      expect.stringContaining('# Project Context')
    );
  });

  it('creates folder templates', async () => {
    const workflow: WorkflowDefinition = {
      ...createMinimalWorkflow(),
      templates: [
        { path: 'docs/', name: 'Documentation', type: 'folder' },
        { path: 'src/', name: 'Source', type: 'folder' },
      ],
    };
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs);

    expect(mockFs.createDirectory).toHaveBeenCalledWith('/parent/MyProject/docs/');
    expect(mockFs.createDirectory).toHaveBeenCalledWith('/parent/MyProject/src/');
  });

  it('creates file templates with static content', async () => {
    const workflow: WorkflowDefinition = {
      ...createMinimalWorkflow(),
      templates: [
        {
          path: 'readme.midlight',
          name: 'README',
          type: 'file',
          contentTemplate: '# {{name}}\n\nWelcome!',
        },
      ],
    };
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs);

    // File content should be wrapped in .midlight JSON format
    const writeCall = (mockFs.writeFile as ReturnType<typeof vi.fn>).mock.calls.find(
      (call) => call[0] === '/parent/MyProject/readme.midlight'
    );
    expect(writeCall).toBeDefined();
    const content = JSON.parse(writeCall![1]);
    expect(content.version).toBe(1);
    expect(content.meta.title).toBe('README');
  });

  it('calls LLM for generateWithLLM templates', async () => {
    const workflow: WorkflowDefinition = {
      ...createMinimalWorkflow(),
      templates: [
        {
          path: 'plan.midlight',
          name: 'Plan',
          type: 'file',
          generateWithLLM: true,
          llmPrompt: 'Create a plan for {{name}}',
        },
      ],
    };
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs, { llmCall: mockLLM });

    expect(mockLLM).toHaveBeenCalledWith('Create a plan for MyProject');
  });

  it('reports progress during execution', async () => {
    const workflow: WorkflowDefinition = {
      ...createMinimalWorkflow(),
      templates: [
        { path: 'file.midlight', name: 'File', type: 'file' },
      ],
    };
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs, {
      onProgress: (p) => progressCalls.push({ ...p }),
    });

    expect(progressCalls.length).toBeGreaterThan(0);
    expect(progressCalls[0].phase).toBe('creating-project');
    expect(progressCalls[progressCalls.length - 1].phase).toBe('complete');
    expect(progressCalls[progressCalls.length - 1].percentComplete).toBe(100);
  });

  it('returns success with created files list', async () => {
    const workflow: WorkflowDefinition = {
      ...createMinimalWorkflow(),
      templates: [
        { path: 'docs/', name: 'Docs', type: 'folder' },
        { path: 'readme.midlight', name: 'README', type: 'file' },
      ],
    };
    const answers: WorkflowAnswers = { name: 'MyProject' };

    const result = await executeWorkflow(workflow, answers, '/parent', mockFs);

    expect(result.success).toBe(true);
    expect(result.projectPath).toBe('/parent/MyProject');
    expect(result.createdFiles).toContain('.project.midlight');
    expect(result.createdFiles).toContain('context.midlight');
    expect(result.createdFiles).toContain('docs/');
    expect(result.createdFiles).toContain('readme.midlight');
    expect(result.failedFiles).toHaveLength(0);
  });

  it('handles file creation errors gracefully', async () => {
    const workflow: WorkflowDefinition = {
      ...createMinimalWorkflow(),
      templates: [
        { path: 'good.midlight', name: 'Good', type: 'file' },
        { path: 'bad.midlight', name: 'Bad', type: 'file' },
      ],
    };
    const answers: WorkflowAnswers = { name: 'MyProject' };

    // Fail only on the 'bad' file
    (mockFs.writeFile as ReturnType<typeof vi.fn>).mockImplementation((path: string) => {
      if (path.includes('bad.midlight')) {
        return Promise.reject(new Error('Write failed'));
      }
      return Promise.resolve();
    });

    const result = await executeWorkflow(workflow, answers, '/parent', mockFs);

    expect(result.success).toBe(false);
    expect(result.createdFiles).toContain('good.midlight');
    expect(result.failedFiles).toHaveLength(1);
    expect(result.failedFiles[0].path).toBe('bad.midlight');
    expect(result.failedFiles[0].error).toBe('Write failed');
  });

  it('handles project directory creation failure', async () => {
    const workflow = createMinimalWorkflow();
    const answers: WorkflowAnswers = { name: 'MyProject' };

    (mockFs.createDirectory as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error('Permission denied')
    );

    const result = await executeWorkflow(workflow, answers, '/parent', mockFs);

    expect(result.success).toBe(false);
    expect(result.error).toBe('Permission denied');
  });

  it('includes workflow source in project config', async () => {
    const workflow: WorkflowDefinition = {
      ...createMinimalWorkflow(),
      id: 'custom-workflow',
    };
    const answers: WorkflowAnswers = { name: 'MyProject' };

    await executeWorkflow(workflow, answers, '/parent', mockFs);

    const configCall = (mockFs.writeFile as ReturnType<typeof vi.fn>).mock.calls.find(
      (call) => call[0].includes('.project.midlight')
    );
    const config = JSON.parse(configCall![1]);
    expect(config.workflowSource).toBe('custom-workflow');
  });
});
