// Document chunking for RAG
// Splits documents into meaningful chunks for embedding

import type { DocumentChunk, ChunkOptions, ChunkMetadata } from './types.js';
import { DEFAULT_CHUNK_OPTIONS } from './types.js';

/**
 * Generates a unique ID for a chunk based on file path and content
 */
export function generateChunkId(filePath: string, chunkIndex: number, content: string): string {
  // Simple hash of path + index + first 50 chars of content
  const key = `${filePath}:${chunkIndex}:${content.slice(0, 50)}`;
  let hash = 0;
  for (let i = 0; i < key.length; i++) {
    const char = key.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash;
  }
  return `chunk_${Math.abs(hash).toString(36)}`;
}

/**
 * Estimates the token count for a piece of text
 * Uses a simple heuristic: ~4 characters per token on average
 */
export function estimateTokens(text: string): number {
  // Rough approximation: 1 token ≈ 4 characters for English text
  // This matches OpenAI's general guidance
  return Math.ceil(text.length / 4);
}

/**
 * Extracts headings from markdown content
 * Returns array of { level, text, offset } objects
 */
interface Heading {
  level: number;
  text: string;
  offset: number;
}

export function extractHeadings(content: string): Heading[] {
  const headings: Heading[] = [];
  const lines = content.split('\n');
  let offset = 0;

  for (const line of lines) {
    const match = line.match(/^(#{1,6})\s+(.+)$/);
    if (match) {
      headings.push({
        level: match[1].length,
        text: match[2].trim(),
        offset,
      });
    }
    offset += line.length + 1; // +1 for newline
  }

  return headings;
}

/**
 * Gets the current heading context for a given character offset
 */
function getHeadingContext(headings: Heading[], offset: number): string | undefined {
  // Find the most recent heading before this offset
  let currentHeading: Heading | undefined;
  for (const heading of headings) {
    if (heading.offset <= offset) {
      currentHeading = heading;
    } else {
      break;
    }
  }
  return currentHeading?.text;
}

/**
 * Splits content into paragraphs/sections for semantic chunking
 */
function splitIntoSections(content: string): string[] {
  // Split on double newlines (paragraph breaks)
  const sections = content.split(/\n\s*\n/);
  return sections.filter(s => s.trim().length > 0);
}

/**
 * Chunks a document into smaller pieces for embedding
 */
export function chunkDocument(
  content: string,
  filePath: string,
  projectPath: string,
  options: Partial<ChunkOptions> = {}
): DocumentChunk[] {
  const opts = { ...DEFAULT_CHUNK_OPTIONS, ...options };
  const chunks: DocumentChunk[] = [];
  const headings = extractHeadings(content);

  // Split into sections first
  const sections = splitIntoSections(content);

  let chunkIndex = 0;
  let charOffset = 0;
  let currentChunkContent = '';
  let currentChunkStart = 0;

  for (const section of sections) {
    const sectionTokens = estimateTokens(section);
    const currentTokens = estimateTokens(currentChunkContent);

    // If adding this section would exceed max, save current chunk and start new
    if (currentTokens + sectionTokens > opts.maxChunkTokens && currentChunkContent.length > 0) {
      // Only create chunk if it meets minimum size
      if (currentTokens >= opts.minChunkTokens) {
        const heading = opts.preserveHeadings ? getHeadingContext(headings, currentChunkStart) : undefined;

        chunks.push({
          id: generateChunkId(filePath, chunkIndex, currentChunkContent),
          projectPath,
          filePath,
          chunkIndex,
          content: currentChunkContent.trim(),
          metadata: {
            heading,
            tokenEstimate: currentTokens,
            charOffset: currentChunkStart,
          },
          createdAt: new Date().toISOString(),
        });

        chunkIndex++;
      }

      // Start new chunk with overlap from previous
      if (opts.overlapTokens > 0) {
        const overlapChars = opts.overlapTokens * 4; // Approximate chars
        currentChunkContent = currentChunkContent.slice(-overlapChars);
        currentChunkStart = charOffset - currentChunkContent.length;
      } else {
        currentChunkContent = '';
        currentChunkStart = charOffset;
      }
    }

    // If section itself is too large, split it by sentences
    if (sectionTokens > opts.maxChunkTokens) {
      const sentences = section.split(/(?<=[.!?])\s+/);
      for (const sentence of sentences) {
        const sentenceTokens = estimateTokens(sentence);
        const newTotal = estimateTokens(currentChunkContent) + sentenceTokens;

        if (newTotal > opts.maxChunkTokens && currentChunkContent.length > 0) {
          const currentTokens = estimateTokens(currentChunkContent);
          if (currentTokens >= opts.minChunkTokens) {
            const heading = opts.preserveHeadings ? getHeadingContext(headings, currentChunkStart) : undefined;

            chunks.push({
              id: generateChunkId(filePath, chunkIndex, currentChunkContent),
              projectPath,
              filePath,
              chunkIndex,
              content: currentChunkContent.trim(),
              metadata: {
                heading,
                tokenEstimate: currentTokens,
                charOffset: currentChunkStart,
              },
              createdAt: new Date().toISOString(),
            });

            chunkIndex++;
          }

          currentChunkContent = '';
          currentChunkStart = charOffset;
        }

        currentChunkContent += (currentChunkContent ? ' ' : '') + sentence;
        charOffset += sentence.length + 1;
      }
    } else {
      currentChunkContent += (currentChunkContent ? '\n\n' : '') + section;
    }

    charOffset += section.length + 2; // +2 for paragraph break
  }

  // Don't forget the last chunk
  if (currentChunkContent.trim().length > 0) {
    const currentTokens = estimateTokens(currentChunkContent);
    if (currentTokens >= opts.minChunkTokens) {
      const heading = opts.preserveHeadings ? getHeadingContext(headings, currentChunkStart) : undefined;

      chunks.push({
        id: generateChunkId(filePath, chunkIndex, currentChunkContent),
        projectPath,
        filePath,
        chunkIndex,
        content: currentChunkContent.trim(),
        metadata: {
          heading,
          tokenEstimate: currentTokens,
          charOffset: currentChunkStart,
        },
        createdAt: new Date().toISOString(),
      });
    }
  }

  return chunks;
}

/**
 * Chunks multiple documents from a project
 */
export function chunkProject(
  documents: { path: string; content: string }[],
  projectPath: string,
  options: Partial<ChunkOptions> = {}
): DocumentChunk[] {
  const allChunks: DocumentChunk[] = [];

  for (const doc of documents) {
    const chunks = chunkDocument(doc.content, doc.path, projectPath, options);
    allChunks.push(...chunks);
  }

  return allChunks;
}

/**
 * Calculates cosine similarity between two vectors
 */
export function cosineSimilarity(a: number[], b: number[]): number {
  if (a.length !== b.length) {
    throw new Error('Vectors must have the same length');
  }

  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i++) {
    dotProduct += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }

  if (normA === 0 || normB === 0) {
    return 0;
  }

  return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
}
