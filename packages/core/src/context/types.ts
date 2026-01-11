// Context document types for parsing and updating

/**
 * A key decision recorded in the context
 */
export interface KeyDecision {
  date: string;
  description: string;
}

/**
 * An open question in the context
 */
export interface OpenQuestion {
  text: string;
  resolved: boolean;
}

/**
 * Parsed structure of a context.midlight document
 */
export interface ParsedContext {
  overview: string;
  currentStatus: string;
  keyDecisions: KeyDecision[];
  openQuestions: OpenQuestion[];
  aiNotes: string;
  rawContent: string;
}

/**
 * Types of updates that can be made to context
 */
export type ContextUpdateType =
  | 'add_decision'
  | 'update_status'
  | 'add_question'
  | 'resolve_question'
  | 'update_overview'
  | 'update_ai_notes';

/**
 * A proposed update to the context document
 */
export interface ContextUpdate {
  type: ContextUpdateType;
  section: 'overview' | 'currentStatus' | 'keyDecisions' | 'openQuestions' | 'aiNotes';
  content: string;
  reason: string;
}

/**
 * Result of context update extraction
 */
export interface ContextUpdateResult {
  updates: ContextUpdate[];
  shouldUpdate: boolean;
  confidence: number;
}

/**
 * Options for context update behavior
 */
export interface ContextUpdateOptions {
  autoUpdate: boolean;
  askBeforeUpdating: boolean;
  showNotifications: boolean;
}
