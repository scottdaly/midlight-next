// Tests for workflow definitions

import { describe, it, expect } from 'vitest';
import {
  weightLossWorkflow,
  bigPurchaseWorkflow,
  bookWritingWorkflow,
  builtInWorkflows,
  getWorkflowById,
  getWorkflowsByCategory,
} from './definitions.js';
import { WORKFLOW_CATEGORIES } from './types.js';
import type { WorkflowDefinition, InterviewStep, TemplateDefinition } from './types.js';

describe('getWorkflowById', () => {
  it('returns weight-loss workflow', () => {
    const workflow = getWorkflowById('weight-loss');
    expect(workflow).toBeDefined();
    expect(workflow!.id).toBe('weight-loss');
    expect(workflow!.name).toBe('Weight Loss Journey');
  });

  it('returns big-purchase workflow', () => {
    const workflow = getWorkflowById('big-purchase');
    expect(workflow).toBeDefined();
    expect(workflow!.id).toBe('big-purchase');
    expect(workflow!.name).toBe('Big Purchase Decision');
  });

  it('returns book-writing workflow', () => {
    const workflow = getWorkflowById('book-writing');
    expect(workflow).toBeDefined();
    expect(workflow!.id).toBe('book-writing');
    expect(workflow!.name).toBe('Book Writing Project');
  });

  it('returns undefined for unknown workflow', () => {
    const workflow = getWorkflowById('unknown-workflow');
    expect(workflow).toBeUndefined();
  });
});

describe('getWorkflowsByCategory', () => {
  it('returns health workflows', () => {
    const workflows = getWorkflowsByCategory('health');
    expect(workflows).toHaveLength(1);
    expect(workflows[0].id).toBe('weight-loss');
  });

  it('returns finance workflows', () => {
    const workflows = getWorkflowsByCategory('finance');
    expect(workflows).toHaveLength(1);
    expect(workflows[0].id).toBe('big-purchase');
  });

  it('returns creative workflows', () => {
    const workflows = getWorkflowsByCategory('creative');
    expect(workflows).toHaveLength(1);
    expect(workflows[0].id).toBe('book-writing');
  });

  it('returns empty array for category with no workflows', () => {
    const workflows = getWorkflowsByCategory('learning');
    expect(workflows).toEqual([]);
  });

  it('returns empty array for unknown category', () => {
    const workflows = getWorkflowsByCategory('unknown');
    expect(workflows).toEqual([]);
  });
});

describe('builtInWorkflows', () => {
  it('contains all three workflows', () => {
    expect(builtInWorkflows).toHaveLength(3);
    const ids = builtInWorkflows.map((w) => w.id);
    expect(ids).toContain('weight-loss');
    expect(ids).toContain('big-purchase');
    expect(ids).toContain('book-writing');
  });

  it('all workflows have unique IDs', () => {
    const ids = builtInWorkflows.map((w) => w.id);
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(ids.length);
  });

  it('all workflows have valid categories', () => {
    const validCategories = WORKFLOW_CATEGORIES.map((c) => c.id);
    for (const workflow of builtInWorkflows) {
      expect(validCategories).toContain(workflow.category);
    }
  });
});

describe('workflow structure validation', () => {
  // Helper to validate common workflow structure
  function validateWorkflowStructure(workflow: WorkflowDefinition, name: string) {
    describe(name, () => {
      it('has required properties', () => {
        expect(workflow.id).toBeTruthy();
        expect(workflow.name).toBeTruthy();
        expect(workflow.description).toBeTruthy();
        expect(workflow.icon).toBeTruthy();
        expect(workflow.category).toBeTruthy();
        expect(workflow.interview).toBeDefined();
        expect(workflow.templates).toBeDefined();
        expect(workflow.contextSections).toBeDefined();
      });

      it('has valid context sections', () => {
        expect(workflow.contextSections.overview).toBeTruthy();
        expect(workflow.contextSections.aiNotes).toBeTruthy();
      });

      it('has at least one interview step', () => {
        expect(workflow.interview.length).toBeGreaterThan(0);
      });

      it('has at least one template', () => {
        expect(workflow.templates.length).toBeGreaterThan(0);
      });

      describe('interview steps', () => {
        it('all steps have unique IDs', () => {
          const ids = workflow.interview.map((s) => s.id);
          const uniqueIds = new Set(ids);
          expect(uniqueIds.size).toBe(ids.length);
        });

        it('all steps have valid types', () => {
          const validTypes = ['text', 'number', 'select', 'multiselect', 'date', 'textarea'];
          for (const step of workflow.interview) {
            expect(validTypes).toContain(step.type);
          }
        });

        it('select/multiselect steps have options', () => {
          for (const step of workflow.interview) {
            if (step.type === 'select' || step.type === 'multiselect') {
              expect(step.options).toBeDefined();
              expect(step.options!.length).toBeGreaterThan(0);
            }
          }
        });

        it('showIf references valid step IDs', () => {
          const stepIds = new Set(workflow.interview.map((s) => s.id));
          for (const step of workflow.interview) {
            if (step.showIf) {
              expect(stepIds).toContain(step.showIf.stepId);
            }
          }
        });
      });

      describe('templates', () => {
        it('all templates have valid types', () => {
          for (const template of workflow.templates) {
            expect(['file', 'folder']).toContain(template.type);
          }
        });

        it('file templates have content source', () => {
          for (const template of workflow.templates) {
            if (template.type === 'file') {
              // Should have either contentTemplate or generateWithLLM
              const hasContent = template.contentTemplate !== undefined;
              const hasLLM = template.generateWithLLM === true;
              expect(hasContent || hasLLM).toBe(true);
            }
          }
        });

        it('LLM templates have prompts', () => {
          for (const template of workflow.templates) {
            if (template.generateWithLLM) {
              expect(template.llmPrompt).toBeTruthy();
            }
          }
        });

        it('folder templates have trailing slash', () => {
          for (const template of workflow.templates) {
            if (template.type === 'folder') {
              expect(template.path.endsWith('/')).toBe(true);
            }
          }
        });
      });
    });
  }

  validateWorkflowStructure(weightLossWorkflow, 'weightLossWorkflow');
  validateWorkflowStructure(bigPurchaseWorkflow, 'bigPurchaseWorkflow');
  validateWorkflowStructure(bookWritingWorkflow, 'bookWritingWorkflow');
});

describe('weightLossWorkflow', () => {
  it('has health category', () => {
    expect(weightLossWorkflow.category).toBe('health');
  });

  it('uses scale emoji icon', () => {
    expect(weightLossWorkflow.icon).toBe('⚖️');
  });

  it('has project name template with goal weight', () => {
    expect(weightLossWorkflow.projectNameTemplate).toContain('{{goal_weight}}');
  });

  it('has green project color', () => {
    expect(weightLossWorkflow.projectColor).toBe('#10b981');
  });

  it('has required weight questions', () => {
    const currentWeight = weightLossWorkflow.interview.find((s) => s.id === 'current_weight');
    const goalWeight = weightLossWorkflow.interview.find((s) => s.id === 'goal_weight');
    expect(currentWeight?.required).toBe(true);
    expect(goalWeight?.required).toBe(true);
  });

  it('has timeline select with options', () => {
    const timeline = weightLossWorkflow.interview.find((s) => s.id === 'timeline');
    expect(timeline?.type).toBe('select');
    expect(timeline?.options).toContain('3 months');
    expect(timeline?.options).toContain('6 months');
  });

  it('has approach multiselect', () => {
    const approach = weightLossWorkflow.interview.find((s) => s.id === 'approach');
    expect(approach?.type).toBe('multiselect');
    expect(approach?.options).toContain('Diet changes');
    expect(approach?.options).toContain('Exercise/workout routine');
  });

  it('has LLM-generated meal and workout plans', () => {
    const mealPlan = weightLossWorkflow.templates.find((t) => t.path === 'meal-plan.midlight');
    const workoutPlan = weightLossWorkflow.templates.find((t) => t.path === 'workout-plan.midlight');
    expect(mealPlan?.generateWithLLM).toBe(true);
    expect(workoutPlan?.generateWithLLM).toBe(true);
  });

  it('has progress log with static template', () => {
    const progressLog = weightLossWorkflow.templates.find((t) => t.path === 'progress-log.midlight');
    expect(progressLog?.contentTemplate).toBeTruthy();
    expect(progressLog?.contentTemplate).toContain('Starting Weight');
    expect(progressLog?.contentTemplate).toContain('Weekly Check-ins');
  });

  it('has recipes folder', () => {
    const recipes = weightLossWorkflow.templates.find((t) => t.path === 'recipes/');
    expect(recipes?.type).toBe('folder');
  });

  it('opens meal plan after creation', () => {
    const mealPlan = weightLossWorkflow.templates.find((t) => t.path === 'meal-plan.midlight');
    expect(mealPlan?.openAfterCreate).toBe(true);
  });
});

describe('bigPurchaseWorkflow', () => {
  it('has finance category', () => {
    expect(bigPurchaseWorkflow.category).toBe('finance');
  });

  it('uses shopping cart emoji icon', () => {
    expect(bigPurchaseWorkflow.icon).toBe('🛒');
  });

  it('has project name template with item type', () => {
    expect(bigPurchaseWorkflow.projectNameTemplate).toContain('{{item_type}}');
  });

  it('has purple project color', () => {
    expect(bigPurchaseWorkflow.projectColor).toBe('#6366f1');
  });

  it('has required item type and budget questions', () => {
    const itemType = bigPurchaseWorkflow.interview.find((s) => s.id === 'item_type');
    const budget = bigPurchaseWorkflow.interview.find((s) => s.id === 'budget');
    expect(itemType?.required).toBe(true);
    expect(budget?.required).toBe(true);
  });

  it('has priorities multiselect with key options', () => {
    const priorities = bigPurchaseWorkflow.interview.find((s) => s.id === 'priorities');
    expect(priorities?.type).toBe('multiselect');
    expect(priorities?.options).toContain('Price/Value');
    expect(priorities?.options).toContain('Quality/Durability');
  });

  it('has three main research files', () => {
    const fileTemplates = bigPurchaseWorkflow.templates.filter((t) => t.type === 'file');
    expect(fileTemplates).toHaveLength(3);
    expect(fileTemplates.map((t) => t.name)).toContain('Research Notes');
    expect(fileTemplates.map((t) => t.name)).toContain('Options Comparison');
    expect(fileTemplates.map((t) => t.name)).toContain('Decision Log');
  });

  it('comparison template has table structure', () => {
    const comparison = bigPurchaseWorkflow.templates.find((t) => t.path === 'comparison.midlight');
    expect(comparison?.contentTemplate).toContain('| Feature |');
    expect(comparison?.contentTemplate).toContain('Final Ranking');
  });
});

describe('bookWritingWorkflow', () => {
  it('has creative category', () => {
    expect(bookWritingWorkflow.category).toBe('creative');
  });

  it('uses book emoji icon', () => {
    expect(bookWritingWorkflow.icon).toBe('📚');
  });

  it('has project name template with working title', () => {
    expect(bookWritingWorkflow.projectNameTemplate).toBe('{{working_title}}');
  });

  it('has purple project color', () => {
    expect(bookWritingWorkflow.projectColor).toBe('#8b5cf6');
  });

  it('has required title and premise questions', () => {
    const title = bookWritingWorkflow.interview.find((s) => s.id === 'working_title');
    const premise = bookWritingWorkflow.interview.find((s) => s.id === 'premise');
    expect(title?.required).toBe(true);
    expect(premise?.required).toBe(true);
  });

  it('has genre select with fiction and non-fiction options', () => {
    const genre = bookWritingWorkflow.interview.find((s) => s.id === 'genre');
    expect(genre?.type).toBe('select');
    expect(genre?.options?.some((o) => o.includes('Fiction'))).toBe(true);
    expect(genre?.options?.some((o) => o.includes('Non-fiction'))).toBe(true);
  });

  it('has target length options', () => {
    const length = bookWritingWorkflow.interview.find((s) => s.id === 'target_length');
    expect(length?.type).toBe('select');
    expect(length?.options).toContain('Short (under 40,000 words)');
    expect(length?.options).toContain('Epic (100,000+ words)');
  });

  it('has LLM-generated outline', () => {
    const outline = bookWritingWorkflow.templates.find((t) => t.path === 'outline.midlight');
    expect(outline?.generateWithLLM).toBe(true);
    expect(outline?.llmPrompt).toContain('{{working_title}}');
    expect(outline?.llmPrompt).toContain('{{premise}}');
  });

  it('has character and world building templates', () => {
    const characters = bookWritingWorkflow.templates.find((t) => t.path === 'characters.midlight');
    const worldBuilding = bookWritingWorkflow.templates.find((t) => t.path === 'world-building.midlight');
    expect(characters?.contentTemplate).toContain('Main Characters');
    expect(worldBuilding?.contentTemplate).toContain('Physical World');
  });

  it('has chapters and drafts folders', () => {
    const folders = bookWritingWorkflow.templates.filter((t) => t.type === 'folder');
    expect(folders.map((f) => f.path)).toContain('chapters/');
    expect(folders.map((f) => f.path)).toContain('drafts/');
  });

  it('opens outline after creation', () => {
    const outline = bookWritingWorkflow.templates.find((t) => t.path === 'outline.midlight');
    expect(outline?.openAfterCreate).toBe(true);
  });
});

describe('context section templates', () => {
  it('weight loss context references interview answers', () => {
    const ctx = weightLossWorkflow.contextSections;
    expect(ctx.overview).toContain('{{current_weight}}');
    expect(ctx.overview).toContain('{{goal_weight}}');
    expect(ctx.aiNotes).toContain('{{challenges}}');
    expect(ctx.aiNotes).toContain('{{motivation}}');
  });

  it('big purchase context references item and budget', () => {
    const ctx = bigPurchaseWorkflow.contextSections;
    expect(ctx.overview).toContain('{{item_type}}');
    expect(ctx.overview).toContain('{{budget}}');
    expect(ctx.aiNotes).toContain('{{priorities}}');
    expect(ctx.aiNotes).toContain('{{deal_breakers}}');
  });

  it('book writing context includes writing guidance', () => {
    const ctx = bookWritingWorkflow.contextSections;
    expect(ctx.aiNotes).toContain('{{working_title}}');
    expect(ctx.aiNotes).toContain('plotting');
    expect(ctx.aiNotes).toContain('character development');
  });

  it('all workflows have initial questions', () => {
    for (const workflow of builtInWorkflows) {
      expect(workflow.contextSections.initialQuestions).toBeDefined();
      expect(workflow.contextSections.initialQuestions!.length).toBeGreaterThan(0);
    }
  });

  it('all workflows have initial decisions with placeholders', () => {
    for (const workflow of builtInWorkflows) {
      expect(workflow.contextSections.initialDecisions).toBeDefined();
      const decisions = workflow.contextSections.initialDecisions!;
      expect(decisions.length).toBeGreaterThan(0);
      // At least one decision should have a placeholder
      const hasPlaceholder = decisions.some((d) => d.includes('{{'));
      expect(hasPlaceholder).toBe(true);
    }
  });
});

describe('WORKFLOW_CATEGORIES', () => {
  it('has required categories for all workflows', () => {
    const categoryIds = WORKFLOW_CATEGORIES.map((c) => c.id);
    expect(categoryIds).toContain('health');
    expect(categoryIds).toContain('finance');
    expect(categoryIds).toContain('creative');
  });

  it('all categories have required properties', () => {
    for (const category of WORKFLOW_CATEGORIES) {
      expect(category.id).toBeTruthy();
      expect(category.name).toBeTruthy();
      expect(category.description).toBeTruthy();
      expect(category.icon).toBeTruthy();
    }
  });

  it('has unique category IDs', () => {
    const ids = WORKFLOW_CATEGORIES.map((c) => c.id);
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(ids.length);
  });
});
