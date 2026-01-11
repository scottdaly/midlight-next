// Context parser - Extracts structured sections from context.midlight

import type { ParsedContext, KeyDecision, OpenQuestion } from './types.js';

/**
 * Section headers we look for in context.midlight
 */
const SECTION_HEADERS = {
  overview: /^##\s*Overview\s*$/im,
  currentStatus: /^##\s*Current\s*Status\s*$/im,
  keyDecisions: /^##\s*Key\s*Decisions\s*$/im,
  openQuestions: /^##\s*Open\s*Questions\s*$/im,
  aiNotes: /^##\s*AI\s*Notes\s*$/im,
};

/**
 * Parses a context.midlight document into structured sections
 */
export function parseContext(markdown: string): ParsedContext {
  const result: ParsedContext = {
    overview: '',
    currentStatus: '',
    keyDecisions: [],
    openQuestions: [],
    aiNotes: '',
    rawContent: markdown,
  };

  // Split into lines for processing
  const lines = markdown.split('\n');
  let currentSection: keyof typeof SECTION_HEADERS | null = null;
  let sectionContent: string[] = [];

  const flushSection = () => {
    if (!currentSection) return;

    const content = sectionContent.join('\n').trim();

    switch (currentSection) {
      case 'overview':
        result.overview = content;
        break;
      case 'currentStatus':
        result.currentStatus = content;
        break;
      case 'keyDecisions':
        result.keyDecisions = parseKeyDecisions(content);
        break;
      case 'openQuestions':
        result.openQuestions = parseOpenQuestions(content);
        break;
      case 'aiNotes':
        result.aiNotes = content;
        break;
    }

    sectionContent = [];
  };

  for (const line of lines) {
    // Check if this line is a section header
    let foundSection: keyof typeof SECTION_HEADERS | null = null;

    for (const [section, pattern] of Object.entries(SECTION_HEADERS)) {
      if (pattern.test(line)) {
        foundSection = section as keyof typeof SECTION_HEADERS;
        break;
      }
    }

    if (foundSection) {
      // Flush previous section
      flushSection();
      currentSection = foundSection;
    } else if (currentSection) {
      // Skip h1 headers (title)
      if (!line.match(/^#\s+/)) {
        sectionContent.push(line);
      }
    }
  }

  // Flush the last section
  flushSection();

  return result;
}

/**
 * Parses key decisions from markdown list
 * Expected format: "- [Date]: [Description]" or "- [Description]"
 */
function parseKeyDecisions(content: string): KeyDecision[] {
  const decisions: KeyDecision[] = [];
  const lines = content.split('\n');

  for (const line of lines) {
    // Match list items: "- " or "* "
    const listMatch = line.match(/^[-*]\s+(.+)$/);
    if (!listMatch) continue;

    const itemContent = listMatch[1].trim();

    // Try to extract date prefix: "[Date]: Description" or "Date: Description"
    const dateMatch = itemContent.match(/^\[?(\d{4}-\d{2}-\d{2}|\d{1,2}\/\d{1,2}\/\d{2,4}|[A-Za-z]+ \d{1,2},? \d{4})\]?:\s*(.+)$/);

    if (dateMatch) {
      decisions.push({
        date: dateMatch[1],
        description: dateMatch[2].trim(),
      });
    } else {
      // No date, use today's date
      decisions.push({
        date: new Date().toISOString().split('T')[0],
        description: itemContent,
      });
    }
  }

  return decisions;
}

/**
 * Parses open questions from markdown task list
 * Expected format: "- [ ] Question" or "- [x] Resolved question"
 */
function parseOpenQuestions(content: string): OpenQuestion[] {
  const questions: OpenQuestion[] = [];
  const lines = content.split('\n');

  for (const line of lines) {
    // Match task list items: "- [ ] " or "- [x] "
    const taskMatch = line.match(/^[-*]\s+\[([ xX])\]\s+(.+)$/);
    if (taskMatch) {
      questions.push({
        resolved: taskMatch[1].toLowerCase() === 'x',
        text: taskMatch[2].trim(),
      });
      continue;
    }

    // Also match regular list items as unresolved questions
    const listMatch = line.match(/^[-*]\s+(.+)$/);
    if (listMatch) {
      const text = listMatch[1].trim();
      // Skip if it looks like a placeholder
      if (text === 'Question 1' || text.startsWith('[')) continue;
      questions.push({
        resolved: false,
        text,
      });
    }
  }

  return questions;
}

/**
 * Serializes a ParsedContext back to markdown
 */
export function serializeContext(context: ParsedContext): string {
  const lines: string[] = ['# Project Context', ''];

  // Overview
  lines.push('## Overview');
  lines.push(context.overview || '[High-level goal and scope of the project]');
  lines.push('');

  // Current Status
  lines.push('## Current Status');
  lines.push(context.currentStatus || '[Where things stand right now]');
  lines.push('');

  // Key Decisions
  lines.push('## Key Decisions');
  if (context.keyDecisions.length === 0) {
    lines.push('- [Date]: [Decision description]');
  } else {
    for (const decision of context.keyDecisions) {
      lines.push(`- [${decision.date}]: ${decision.description}`);
    }
  }
  lines.push('');

  // Open Questions
  lines.push('## Open Questions');
  if (context.openQuestions.length === 0) {
    lines.push('- [ ] Question 1');
  } else {
    for (const question of context.openQuestions) {
      const checkbox = question.resolved ? '[x]' : '[ ]';
      lines.push(`- ${checkbox} ${question.text}`);
    }
  }
  lines.push('');

  // AI Notes
  lines.push('## AI Notes');
  lines.push(context.aiNotes || '[Meta-instructions for how the AI should behave in this project]');
  lines.push('');

  return lines.join('\n');
}

/**
 * Applies updates to a parsed context and returns the new context
 */
export function applyUpdatesToContext(
  context: ParsedContext,
  updates: { type: string; section: string; content: string }[]
): ParsedContext {
  const newContext = { ...context };

  for (const update of updates) {
    switch (update.type) {
      case 'add_decision':
        newContext.keyDecisions = [
          ...newContext.keyDecisions,
          {
            date: new Date().toISOString().split('T')[0],
            description: update.content,
          },
        ];
        break;

      case 'update_status':
        newContext.currentStatus = update.content;
        break;

      case 'add_question':
        newContext.openQuestions = [
          ...newContext.openQuestions,
          { text: update.content, resolved: false },
        ];
        break;

      case 'resolve_question':
        newContext.openQuestions = newContext.openQuestions.map((q) =>
          q.text.toLowerCase().includes(update.content.toLowerCase())
            ? { ...q, resolved: true }
            : q
        );
        break;

      case 'update_overview':
        newContext.overview = update.content;
        break;

      case 'update_ai_notes':
        newContext.aiNotes = update.content;
        break;
    }
  }

  return newContext;
}
