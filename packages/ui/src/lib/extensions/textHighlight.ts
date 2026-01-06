/**
 * Text highlight extension for Tiptap
 * Re-exports the official @tiptap/extension-highlight with multicolor support
 * Colors are stored as hex values (e.g., #ffff00) in inline styles
 */

import Highlight from '@tiptap/extension-highlight';

// Export configured highlight with multicolor support
export const TextHighlight = Highlight.configure({
  multicolor: true,
});
