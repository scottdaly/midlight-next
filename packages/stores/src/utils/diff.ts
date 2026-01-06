/**
 * Diff computation utilities for Tiptap documents
 * Used for visual diff display in staged edits
 */

export interface DiffSegment {
  type: 'unchanged' | 'added' | 'removed';
  text: string;
}

interface LCSMatch {
  oldIndex: number;
  newIndex: number;
}

/**
 * Computes LCS (Longest Common Subsequence) for two arrays
 */
function computeLCS(oldArr: string[], newArr: string[]): LCSMatch[] {
  const m = oldArr.length;
  const n = newArr.length;

  const dp: number[][] = Array(m + 1)
    .fill(null)
    .map(() => Array(n + 1).fill(0));

  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (oldArr[i - 1] === newArr[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1;
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
      }
    }
  }

  const matches: LCSMatch[] = [];
  let i = m;
  let j = n;

  while (i > 0 && j > 0) {
    if (oldArr[i - 1] === newArr[j - 1]) {
      matches.unshift({ oldIndex: i - 1, newIndex: j - 1 });
      i--;
      j--;
    } else if (dp[i - 1][j] > dp[i][j - 1]) {
      i--;
    } else {
      j--;
    }
  }

  return matches;
}

/**
 * Computes word-level diff between original and new text
 */
export function computeWordDiff(original: string, suggested: string): DiffSegment[] {
  const originalWords = original.split(/(\s+)/);
  const suggestedWords = suggested.split(/(\s+)/);
  const result: DiffSegment[] = [];

  const lcs = computeLCS(originalWords, suggestedWords);

  let origIdx = 0;
  let suggIdx = 0;

  for (const match of lcs) {
    // Add removed words
    while (origIdx < match.oldIndex) {
      result.push({ type: 'removed', text: originalWords[origIdx] });
      origIdx++;
    }

    // Add added words
    while (suggIdx < match.newIndex) {
      result.push({ type: 'added', text: suggestedWords[suggIdx] });
      suggIdx++;
    }

    // Add matching word
    result.push({ type: 'unchanged', text: originalWords[origIdx] });
    origIdx++;
    suggIdx++;
  }

  // Remaining removed words
  while (origIdx < originalWords.length) {
    result.push({ type: 'removed', text: originalWords[origIdx] });
    origIdx++;
  }

  // Remaining added words
  while (suggIdx < suggestedWords.length) {
    result.push({ type: 'added', text: suggestedWords[suggIdx] });
    suggIdx++;
  }

  return result;
}

/**
 * Merges consecutive segments of the same type
 */
export function mergeConsecutiveSegments(segments: DiffSegment[]): DiffSegment[] {
  if (segments.length === 0) return [];

  const merged: DiffSegment[] = [];
  let current = { ...segments[0] };

  for (let i = 1; i < segments.length; i++) {
    if (segments[i].type === current.type) {
      current.text += segments[i].text;
    } else {
      merged.push(current);
      current = { ...segments[i] };
    }
  }
  merged.push(current);

  return merged;
}

/**
 * Creates a Tiptap content array with diff marks applied
 * Returns content nodes suitable for inserting into a Tiptap document
 */
export function createDiffContent(
  originalText: string,
  newText: string
): { type: string; text?: string; marks?: { type: string }[] }[] {
  const segments = computeWordDiff(originalText, newText);
  const merged = mergeConsecutiveSegments(segments);

  return merged.map((segment) => {
    if (segment.type === 'unchanged') {
      return {
        type: 'text',
        text: segment.text,
      };
    } else if (segment.type === 'removed') {
      return {
        type: 'text',
        text: segment.text,
        marks: [{ type: 'diffRemoved' }],
      };
    } else {
      // added
      return {
        type: 'text',
        text: segment.text,
        marks: [{ type: 'diffAdded' }],
      };
    }
  });
}

/**
 * Extracts plain text from a Tiptap document
 */
export function extractTextFromTiptap(doc: TiptapDoc): string {
  if (!doc || !doc.content) return '';

  const extractFromNode = (node: TiptapNode): string => {
    if (node.type === 'text') {
      return node.text || '';
    }
    if (node.content) {
      return node.content.map(extractFromNode).join('');
    }
    // Add newlines for block-level elements
    if (['paragraph', 'heading', 'bulletList', 'orderedList', 'listItem', 'blockquote'].includes(node.type)) {
      return '\n';
    }
    return '';
  };

  return doc.content.map(extractFromNode).join('\n').trim();
}

// Type definitions for Tiptap structures
interface TiptapMark {
  type: string;
  attrs?: Record<string, unknown>;
}

interface TiptapNode {
  type: string;
  text?: string;
  content?: TiptapNode[];
  marks?: TiptapMark[];
  attrs?: Record<string, unknown>;
}

interface TiptapDoc {
  type: 'doc';
  content?: TiptapNode[];
}

/**
 * Calculate similarity between two strings (0 to 1)
 * Uses a simple approach based on common words
 */
function calculateSimilarity(str1: string, str2: string): number {
  if (str1 === str2) return 1;
  if (!str1 || !str2) return 0;

  const words1 = new Set(str1.toLowerCase().split(/\s+/).filter(w => w.length > 0));
  const words2 = new Set(str2.toLowerCase().split(/\s+/).filter(w => w.length > 0));

  if (words1.size === 0 && words2.size === 0) return 1;
  if (words1.size === 0 || words2.size === 0) return 0;

  let common = 0;
  for (const word of words1) {
    if (words2.has(word)) common++;
  }

  // Jaccard similarity
  const union = words1.size + words2.size - common;
  return union > 0 ? common / union : 0;
}

// Threshold below which we show full line replacement instead of word diff
const SIMILARITY_THRESHOLD = 0.3;

/**
 * Creates a merged Tiptap document showing diff between original and new content
 * Removed text gets diffRemoved marks, added text gets diffAdded marks
 *
 * Smart diffing: When paragraphs are substantially different (low similarity),
 * shows full line deletions/additions instead of noisy word-level diffs
 */
export function createMergedDiffDocument(
  originalDoc: TiptapDoc,
  newDoc: TiptapDoc
): TiptapDoc {
  const originalParagraphs = originalDoc.content || [];
  const newParagraphs = newDoc.content || [];

  const mergedContent: TiptapNode[] = [];

  // Use LCS on paragraphs to find best alignment
  const origTexts = originalParagraphs.map(p => extractTextFromNode(p));
  const newTexts = newParagraphs.map(p => extractTextFromNode(p));

  // Find matching paragraphs using LCS
  const matches = findParagraphMatches(origTexts, newTexts);

  let origIdx = 0;
  let newIdx = 0;

  for (const match of matches) {
    // Add removed paragraphs before this match
    while (origIdx < match.origIndex) {
      mergedContent.push(markParagraphAs(originalParagraphs[origIdx], 'diffRemoved'));
      origIdx++;
    }

    // Add new paragraphs before this match
    while (newIdx < match.newIndex) {
      mergedContent.push(markParagraphAs(newParagraphs[newIdx], 'diffAdded'));
      newIdx++;
    }

    // Handle the matched pair
    const origPara = originalParagraphs[origIdx];
    const newPara = newParagraphs[newIdx];
    const origText = origTexts[origIdx];
    const newText = newTexts[newIdx];

    if (origText === newText) {
      // Identical - keep as is
      mergedContent.push(origPara);
    } else {
      // Check similarity to decide diff strategy
      const similarity = calculateSimilarity(origText, newText);

      if (similarity < SIMILARITY_THRESHOLD) {
        // Low similarity - show as full line replacement (cleaner)
        mergedContent.push(markParagraphAs(origPara, 'diffRemoved'));
        mergedContent.push(markParagraphAs(newPara, 'diffAdded'));
      } else {
        // High similarity - show word-level diff (more precise)
        mergedContent.push(createDiffParagraph(origPara, origText, newText));
      }
    }

    origIdx++;
    newIdx++;
  }

  // Add remaining removed paragraphs
  while (origIdx < originalParagraphs.length) {
    mergedContent.push(markParagraphAs(originalParagraphs[origIdx], 'diffRemoved'));
    origIdx++;
  }

  // Add remaining new paragraphs
  while (newIdx < newParagraphs.length) {
    mergedContent.push(markParagraphAs(newParagraphs[newIdx], 'diffAdded'));
    newIdx++;
  }

  return {
    type: 'doc',
    content: mergedContent,
  };
}

/**
 * Find best matching paragraphs using LCS approach
 */
interface ParagraphMatch {
  origIndex: number;
  newIndex: number;
}

function findParagraphMatches(origTexts: string[], newTexts: string[]): ParagraphMatch[] {
  const m = origTexts.length;
  const n = newTexts.length;

  // Build similarity matrix
  const isSimilar = (i: number, j: number): boolean => {
    if (origTexts[i] === newTexts[j]) return true;
    // Consider paragraphs "matching" if they have reasonable similarity
    return calculateSimilarity(origTexts[i], newTexts[j]) >= SIMILARITY_THRESHOLD;
  };

  // LCS with similarity-based matching
  const dp: number[][] = Array(m + 1).fill(null).map(() => Array(n + 1).fill(0));

  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (isSimilar(i - 1, j - 1)) {
        dp[i][j] = dp[i - 1][j - 1] + 1;
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
      }
    }
  }

  // Backtrack to find matches
  const matches: ParagraphMatch[] = [];
  let i = m;
  let j = n;

  while (i > 0 && j > 0) {
    if (isSimilar(i - 1, j - 1)) {
      matches.unshift({ origIndex: i - 1, newIndex: j - 1 });
      i--;
      j--;
    } else if (dp[i - 1][j] > dp[i][j - 1]) {
      i--;
    } else {
      j--;
    }
  }

  return matches;
}

/**
 * Extract text from a single node
 */
function extractTextFromNode(node: TiptapNode): string {
  if (node.type === 'text') {
    return node.text || '';
  }
  if (node.content) {
    return node.content.map(extractTextFromNode).join('');
  }
  return '';
}

/**
 * Mark all text in a paragraph with a specific mark type
 */
function markParagraphAs(para: TiptapNode, markType: string): TiptapNode {
  if (para.type === 'text') {
    return {
      ...para,
      marks: [...(para.marks || []), { type: markType }],
    };
  }

  if (para.content) {
    return {
      ...para,
      content: para.content.map(child => markParagraphAs(child, markType)),
    };
  }

  return para;
}

/**
 * Create a paragraph with word-level diff marks
 */
function createDiffParagraph(
  origPara: TiptapNode,
  origText: string,
  newText: string
): TiptapNode {
  const segments = computeWordDiff(origText, newText);
  const merged = mergeConsecutiveSegments(segments);

  const content: TiptapNode[] = merged
    .filter(seg => seg.text) // Filter out empty segments
    .map(segment => {
      if (segment.type === 'unchanged') {
        return {
          type: 'text',
          text: segment.text,
        };
      } else if (segment.type === 'removed') {
        return {
          type: 'text',
          text: segment.text,
          marks: [{ type: 'diffRemoved' }],
        };
      } else {
        return {
          type: 'text',
          text: segment.text,
          marks: [{ type: 'diffAdded' }],
        };
      }
    });

  return {
    type: origPara.type, // Keep the original node type (paragraph, heading, etc.)
    attrs: origPara.attrs, // Keep any attributes (like heading level)
    content: content.length > 0 ? content : [{ type: 'text', text: '' }],
  };
}
