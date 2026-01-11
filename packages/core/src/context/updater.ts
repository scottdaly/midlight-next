// Context updater - Extracts and applies updates to context documents

import type {
  ContextUpdate,
  ContextUpdateResult,
  ParsedContext,
} from './types.js';
import { parseContext, serializeContext, applyUpdatesToContext } from './parser.js';

/**
 * Prompt template for extracting context updates from an AI response
 */
const EXTRACTION_PROMPT = `You are analyzing a conversation to identify updates that should be made to a project's context document.

The context document has these sections:
- Overview: High-level goal and scope
- Current Status: Where things stand now
- Key Decisions: Important choices made (with dates)
- Open Questions: Unresolved items
- AI Notes: Meta-instructions for AI behavior

CURRENT CONTEXT:
{context}

RECENT CONVERSATION:
User: {userMessage}
Assistant: {assistantResponse}

Analyze if any updates should be made to the context. Only suggest updates for:
1. Decisions the user has clearly made
2. Significant status changes
3. New questions that arose
4. Questions that were resolved
5. Changes to project scope/goals

Be conservative - only suggest updates for clear, meaningful changes. Don't update for casual conversation or minor details.

Respond ONLY with a JSON object in this exact format (no markdown, no explanation):
{
  "shouldUpdate": true/false,
  "confidence": 0.0-1.0,
  "updates": [
    {
      "type": "add_decision" | "update_status" | "add_question" | "resolve_question" | "update_overview" | "update_ai_notes",
      "section": "overview" | "currentStatus" | "keyDecisions" | "openQuestions" | "aiNotes",
      "content": "the update content",
      "reason": "why this update is needed"
    }
  ]
}

If no updates are needed, respond with: {"shouldUpdate": false, "confidence": 1.0, "updates": []}`;

/**
 * Extracts potential context updates from a conversation
 * This is a pure function that returns the prompt - actual LLM call is done by caller
 */
export function buildExtractionPrompt(
  currentContext: string,
  userMessage: string,
  assistantResponse: string
): string {
  return EXTRACTION_PROMPT
    .replace('{context}', currentContext)
    .replace('{userMessage}', userMessage)
    .replace('{assistantResponse}', assistantResponse);
}

/**
 * Parses the LLM response to extract context updates
 */
export function parseExtractionResponse(response: string): ContextUpdateResult {
  try {
    // Try to extract JSON from the response (handle markdown code blocks)
    let jsonStr = response.trim();

    // Remove markdown code blocks if present
    const jsonMatch = jsonStr.match(/```(?:json)?\s*([\s\S]*?)```/);
    if (jsonMatch) {
      jsonStr = jsonMatch[1].trim();
    }

    const parsed = JSON.parse(jsonStr);

    // Validate structure
    if (typeof parsed.shouldUpdate !== 'boolean') {
      return { shouldUpdate: false, confidence: 0, updates: [] };
    }

    const updates: ContextUpdate[] = [];

    if (Array.isArray(parsed.updates)) {
      for (const update of parsed.updates) {
        if (
          update.type &&
          update.section &&
          update.content &&
          update.reason
        ) {
          updates.push({
            type: update.type,
            section: update.section,
            content: update.content,
            reason: update.reason,
          });
        }
      }
    }

    return {
      shouldUpdate: parsed.shouldUpdate && updates.length > 0,
      confidence: typeof parsed.confidence === 'number' ? parsed.confidence : 0.5,
      updates,
    };
  } catch (error) {
    console.warn('[ContextUpdater] Failed to parse extraction response:', error);
    return { shouldUpdate: false, confidence: 0, updates: [] };
  }
}

/**
 * Applies updates to a context document and returns the new markdown
 */
export function applyContextUpdates(
  contextMarkdown: string,
  updates: ContextUpdate[]
): string {
  const parsed = parseContext(contextMarkdown);
  const updated = applyUpdatesToContext(parsed, updates);
  return serializeContext(updated);
}

/**
 * Creates a human-readable summary of proposed updates
 */
export function summarizeUpdates(updates: ContextUpdate[]): string {
  if (updates.length === 0) return 'No updates proposed';

  const summaries: string[] = [];

  for (const update of updates) {
    switch (update.type) {
      case 'add_decision':
        summaries.push(`Add decision: "${truncate(update.content, 50)}"`);
        break;
      case 'update_status':
        summaries.push(`Update status to: "${truncate(update.content, 50)}"`);
        break;
      case 'add_question':
        summaries.push(`Add question: "${truncate(update.content, 50)}"`);
        break;
      case 'resolve_question':
        summaries.push(`Mark resolved: "${truncate(update.content, 50)}"`);
        break;
      case 'update_overview':
        summaries.push(`Update overview`);
        break;
      case 'update_ai_notes':
        summaries.push(`Update AI notes`);
        break;
    }
  }

  return summaries.join('\n');
}

/**
 * Truncates a string to a maximum length
 */
function truncate(str: string, maxLength: number): string {
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength - 3) + '...';
}

/**
 * Determines if an update should be applied based on confidence threshold
 */
export function shouldApplyUpdate(
  result: ContextUpdateResult,
  minConfidence: number = 0.7
): boolean {
  return result.shouldUpdate && result.confidence >= minConfidence;
}

/**
 * Filters updates to only include high-confidence, safe updates
 */
export function filterSafeUpdates(updates: ContextUpdate[]): ContextUpdate[] {
  // Only allow additive updates automatically
  const safeTypes = ['add_decision', 'add_question', 'resolve_question'];
  return updates.filter((u) => safeTypes.includes(u.type));
}

/**
 * Creates a diff preview between old and new context
 */
export function createContextDiff(
  oldContext: string,
  newContext: string
): { section: string; oldValue: string; newValue: string }[] {
  const oldParsed = parseContext(oldContext);
  const newParsed = parseContext(newContext);

  const diffs: { section: string; oldValue: string; newValue: string }[] = [];

  if (oldParsed.overview !== newParsed.overview) {
    diffs.push({
      section: 'Overview',
      oldValue: oldParsed.overview,
      newValue: newParsed.overview,
    });
  }

  if (oldParsed.currentStatus !== newParsed.currentStatus) {
    diffs.push({
      section: 'Current Status',
      oldValue: oldParsed.currentStatus,
      newValue: newParsed.currentStatus,
    });
  }

  // Check for new decisions
  const oldDecisionTexts = new Set(oldParsed.keyDecisions.map((d) => d.description));
  const newDecisions = newParsed.keyDecisions.filter((d) => !oldDecisionTexts.has(d.description));
  if (newDecisions.length > 0) {
    diffs.push({
      section: 'Key Decisions',
      oldValue: '',
      newValue: newDecisions.map((d) => `[${d.date}]: ${d.description}`).join('\n'),
    });
  }

  // Check for resolved questions
  const oldQuestionMap = new Map(oldParsed.openQuestions.map((q) => [q.text, q.resolved]));
  const resolvedQuestions = newParsed.openQuestions.filter(
    (q) => q.resolved && oldQuestionMap.get(q.text) === false
  );
  if (resolvedQuestions.length > 0) {
    diffs.push({
      section: 'Open Questions (Resolved)',
      oldValue: resolvedQuestions.map((q) => `[ ] ${q.text}`).join('\n'),
      newValue: resolvedQuestions.map((q) => `[x] ${q.text}`).join('\n'),
    });
  }

  // Check for new questions
  const oldQuestionTexts = new Set(oldParsed.openQuestions.map((q) => q.text));
  const newQuestions = newParsed.openQuestions.filter((q) => !oldQuestionTexts.has(q.text));
  if (newQuestions.length > 0) {
    diffs.push({
      section: 'Open Questions (New)',
      oldValue: '',
      newValue: newQuestions.map((q) => `[ ] ${q.text}`).join('\n'),
    });
  }

  if (oldParsed.aiNotes !== newParsed.aiNotes) {
    diffs.push({
      section: 'AI Notes',
      oldValue: oldParsed.aiNotes,
      newValue: newParsed.aiNotes,
    });
  }

  return diffs;
}
