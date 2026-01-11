// Context parser tests

import { describe, it, expect } from 'vitest';
import { parseContext, serializeContext, applyUpdatesToContext } from './parser.js';

describe('parseContext', () => {
  it('should parse a well-formatted context document', () => {
    const markdown = `# Project Context

## Overview
Building a task management app with real-time sync.

## Current Status
MVP complete, working on polish.

## Key Decisions
- [2025-01-05]: Using SQLite for local storage
- [2025-01-08]: Switched to Tailwind for styling

## Open Questions
- [ ] Should we add dark mode?
- [x] What database to use? (answered: SQLite)
- [ ] Mobile app priority?

## AI Notes
Focus on simplicity and user experience.
`;

    const result = parseContext(markdown);

    expect(result.overview).toBe('Building a task management app with real-time sync.');
    expect(result.currentStatus).toBe('MVP complete, working on polish.');
    expect(result.keyDecisions).toHaveLength(2);
    expect(result.keyDecisions[0].date).toBe('2025-01-05');
    expect(result.keyDecisions[0].description).toBe('Using SQLite for local storage');
    expect(result.openQuestions).toHaveLength(3);
    expect(result.openQuestions[0].text).toBe('Should we add dark mode?');
    expect(result.openQuestions[0].resolved).toBe(false);
    expect(result.openQuestions[1].resolved).toBe(true);
    expect(result.aiNotes).toBe('Focus on simplicity and user experience.');
  });

  it('should handle empty sections', () => {
    const markdown = `# Project Context

## Overview

## Current Status

## Key Decisions

## Open Questions

## AI Notes
`;

    const result = parseContext(markdown);

    expect(result.overview).toBe('');
    expect(result.currentStatus).toBe('');
    expect(result.keyDecisions).toHaveLength(0);
    expect(result.openQuestions).toHaveLength(0);
    expect(result.aiNotes).toBe('');
  });
});

describe('serializeContext', () => {
  it('should serialize a parsed context back to markdown', () => {
    const context = {
      overview: 'Test project',
      currentStatus: 'In progress',
      keyDecisions: [
        { date: '2025-01-10', description: 'Use TypeScript' },
      ],
      openQuestions: [
        { text: 'What framework?', resolved: false },
        { text: 'Database choice', resolved: true },
      ],
      aiNotes: 'Be helpful',
      rawContent: '',
    };

    const markdown = serializeContext(context);

    expect(markdown).toContain('## Overview');
    expect(markdown).toContain('Test project');
    expect(markdown).toContain('## Current Status');
    expect(markdown).toContain('In progress');
    expect(markdown).toContain('## Key Decisions');
    expect(markdown).toContain('[2025-01-10]: Use TypeScript');
    expect(markdown).toContain('## Open Questions');
    expect(markdown).toContain('- [ ] What framework?');
    expect(markdown).toContain('- [x] Database choice');
    expect(markdown).toContain('## AI Notes');
    expect(markdown).toContain('Be helpful');
  });
});

describe('applyUpdatesToContext', () => {
  it('should add a new decision', () => {
    const context = {
      overview: 'Test',
      currentStatus: 'Active',
      keyDecisions: [],
      openQuestions: [],
      aiNotes: '',
      rawContent: '',
    };

    const updates = [
      { type: 'add_decision', section: 'keyDecisions', content: 'Use React' },
    ];

    const result = applyUpdatesToContext(context, updates);

    expect(result.keyDecisions).toHaveLength(1);
    expect(result.keyDecisions[0].description).toBe('Use React');
  });

  it('should update status', () => {
    const context = {
      overview: 'Test',
      currentStatus: 'Planning',
      keyDecisions: [],
      openQuestions: [],
      aiNotes: '',
      rawContent: '',
    };

    const updates = [
      { type: 'update_status', section: 'currentStatus', content: 'In development' },
    ];

    const result = applyUpdatesToContext(context, updates);

    expect(result.currentStatus).toBe('In development');
  });

  it('should add a new question', () => {
    const context = {
      overview: 'Test',
      currentStatus: 'Active',
      keyDecisions: [],
      openQuestions: [],
      aiNotes: '',
      rawContent: '',
    };

    const updates = [
      { type: 'add_question', section: 'openQuestions', content: 'What about testing?' },
    ];

    const result = applyUpdatesToContext(context, updates);

    expect(result.openQuestions).toHaveLength(1);
    expect(result.openQuestions[0].text).toBe('What about testing?');
    expect(result.openQuestions[0].resolved).toBe(false);
  });

  it('should resolve a question', () => {
    const context = {
      overview: 'Test',
      currentStatus: 'Active',
      keyDecisions: [],
      openQuestions: [
        { text: 'What database to use?', resolved: false },
      ],
      aiNotes: '',
      rawContent: '',
    };

    const updates = [
      { type: 'resolve_question', section: 'openQuestions', content: 'database' },
    ];

    const result = applyUpdatesToContext(context, updates);

    expect(result.openQuestions[0].resolved).toBe(true);
  });
});
