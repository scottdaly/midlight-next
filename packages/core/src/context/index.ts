// Context module - Parsing and updating context documents

export type {
  KeyDecision,
  OpenQuestion,
  ParsedContext,
  ContextUpdateType,
  ContextUpdate,
  ContextUpdateResult,
  ContextUpdateOptions,
} from './types.js';

export {
  parseContext,
  serializeContext,
  applyUpdatesToContext,
} from './parser.js';

export {
  buildExtractionPrompt,
  parseExtractionResponse,
  applyContextUpdates,
  summarizeUpdates,
  shouldApplyUpdate,
  filterSafeUpdates,
  createContextDiff,
} from './updater.js';
