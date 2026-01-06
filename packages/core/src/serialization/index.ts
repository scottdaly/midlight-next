// @midlight/core/serialization - Document serialization
// Tiptap JSON <-> Markdown + Sidecar conversion

import type { TiptapDocument, SidecarDocument, SidecarMeta, BlockFormatting, SpanFormatting } from '../types/index.js';

/**
 * Creates an empty sidecar document with default metadata
 */
export function createEmptySidecar(): SidecarDocument {
  const now = new Date().toISOString();
  return {
    version: 1,
    meta: {
      created: now,
      modified: now,
    },
    document: {},
    blocks: {},
    spans: {},
    images: {},
  };
}

/**
 * Updates sidecar metadata timestamps
 */
export function updateSidecarMeta(sidecar: SidecarDocument, updates: Partial<SidecarMeta>): SidecarDocument {
  return {
    ...sidecar,
    meta: {
      ...sidecar.meta,
      ...updates,
      modified: new Date().toISOString(),
    },
  };
}

/**
 * Calculates word count from text content
 */
export function countWords(text: string): number {
  return text
    .trim()
    .split(/\s+/)
    .filter((word) => word.length > 0).length;
}

/**
 * Estimates reading time in minutes (200 words per minute)
 */
export function estimateReadingTime(wordCount: number): number {
  return Math.ceil(wordCount / 200);
}

// DocumentSerializer and DocumentDeserializer will be ported from the existing codebase
// These are placeholder exports for now

export { DocumentSerializer } from './documentSerializer.js';
export { DocumentDeserializer } from './documentDeserializer.js';
