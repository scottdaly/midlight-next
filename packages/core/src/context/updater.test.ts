// Context updater tests

import { describe, it, expect } from 'vitest';
import {
  buildExtractionPrompt,
  parseExtractionResponse,
  applyContextUpdates,
  summarizeUpdates,
  filterSafeUpdates,
  createContextDiff,
} from './updater.js';

describe('buildExtractionPrompt', () => {
  it('should build a prompt with context, user message, and assistant response', () => {
    const context = '## Overview\nTest project';
    const userMessage = 'Let us use React';
    const assistantResponse = 'Great choice! React is excellent for building UIs.';

    const prompt = buildExtractionPrompt(context, userMessage, assistantResponse);

    expect(prompt).toContain('## Overview\nTest project');
    expect(prompt).toContain('Let us use React');
    expect(prompt).toContain('Great choice! React is excellent for building UIs.');
  });
});

describe('parseExtractionResponse', () => {
  it('should parse a valid JSON response', () => {
    const response = `{
      "shouldUpdate": true,
      "confidence": 0.85,
      "updates": [
        {
          "type": "add_decision",
          "section": "keyDecisions",
          "content": "Use React for frontend",
          "reason": "User decided on React"
        }
      ]
    }`;

    const result = parseExtractionResponse(response);

    expect(result.shouldUpdate).toBe(true);
    expect(result.confidence).toBe(0.85);
    expect(result.updates).toHaveLength(1);
    expect(result.updates[0].type).toBe('add_decision');
    expect(result.updates[0].content).toBe('Use React for frontend');
  });

  it('should handle markdown code blocks', () => {
    const response = '```json\n{"shouldUpdate": false, "confidence": 1.0, "updates": []}\n```';

    const result = parseExtractionResponse(response);

    expect(result.shouldUpdate).toBe(false);
    expect(result.confidence).toBe(1.0);
    expect(result.updates).toHaveLength(0);
  });

  it('should handle invalid JSON gracefully', () => {
    const response = 'This is not JSON';

    const result = parseExtractionResponse(response);

    expect(result.shouldUpdate).toBe(false);
    expect(result.confidence).toBe(0);
    expect(result.updates).toHaveLength(0);
  });

  it('should handle empty updates array', () => {
    const response = '{"shouldUpdate": false, "confidence": 1.0, "updates": []}';

    const result = parseExtractionResponse(response);

    expect(result.shouldUpdate).toBe(false);
    expect(result.updates).toHaveLength(0);
  });
});

describe('applyContextUpdates', () => {
  it('should apply updates to context markdown', () => {
    const contextMarkdown = `# Project Context

## Overview
Test project

## Current Status
Planning

## Key Decisions

## Open Questions

## AI Notes
`;

    const updates = [
      { type: 'add_decision' as const, section: 'keyDecisions' as const, content: 'Use TypeScript', reason: '' },
    ];

    const result = applyContextUpdates(contextMarkdown, updates);

    expect(result).toContain('Use TypeScript');
  });
});

describe('summarizeUpdates', () => {
  it('should create a readable summary', () => {
    const updates = [
      { type: 'add_decision' as const, section: 'keyDecisions' as const, content: 'Use React', reason: '' },
      { type: 'update_status' as const, section: 'currentStatus' as const, content: 'In progress', reason: '' },
    ];

    const summary = summarizeUpdates(updates);

    expect(summary).toContain('Add decision');
    expect(summary).toContain('Use React');
    expect(summary).toContain('Update status');
    expect(summary).toContain('In progress');
  });

  it('should return no updates message for empty array', () => {
    const summary = summarizeUpdates([]);

    expect(summary).toBe('No updates proposed');
  });
});

describe('filterSafeUpdates', () => {
  it('should only allow additive updates', () => {
    const updates = [
      { type: 'add_decision' as const, section: 'keyDecisions' as const, content: 'Use React', reason: '' },
      { type: 'update_status' as const, section: 'currentStatus' as const, content: 'In progress', reason: '' },
      { type: 'add_question' as const, section: 'openQuestions' as const, content: 'Testing?', reason: '' },
      { type: 'update_overview' as const, section: 'overview' as const, content: 'New overview', reason: '' },
    ];

    const safe = filterSafeUpdates(updates);

    expect(safe).toHaveLength(2);
    expect(safe[0].type).toBe('add_decision');
    expect(safe[1].type).toBe('add_question');
  });
});

describe('createContextDiff', () => {
  it('should detect changes in overview', () => {
    const oldContext = `# Project Context

## Overview
Old overview

## Current Status
Active

## Key Decisions

## Open Questions

## AI Notes
`;

    const newContext = `# Project Context

## Overview
New overview

## Current Status
Active

## Key Decisions

## Open Questions

## AI Notes
`;

    const diffs = createContextDiff(oldContext, newContext);

    expect(diffs).toHaveLength(1);
    expect(diffs[0].section).toBe('Overview');
    expect(diffs[0].oldValue).toBe('Old overview');
    expect(diffs[0].newValue).toBe('New overview');
  });

  it('should detect new decisions', () => {
    const oldContext = `# Project Context

## Overview
Test

## Current Status
Active

## Key Decisions

## Open Questions

## AI Notes
`;

    const newContext = `# Project Context

## Overview
Test

## Current Status
Active

## Key Decisions
- [2025-01-10]: Use TypeScript

## Open Questions

## AI Notes
`;

    const diffs = createContextDiff(oldContext, newContext);

    expect(diffs.some(d => d.section === 'Key Decisions')).toBe(true);
  });
});
