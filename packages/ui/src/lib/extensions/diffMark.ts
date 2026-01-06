import { Mark, mergeAttributes } from '@tiptap/core';

/**
 * Mark for text that was added in a diff
 */
export const DiffAdded = Mark.create({
  name: 'diffAdded',

  addAttributes() {
    return {};
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-diff-added]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        'data-diff-added': '',
        class: 'diff-added',
        style: 'background-color: rgba(34, 197, 94, 0.3); color: inherit;',
      }),
      0,
    ];
  },
});

/**
 * Mark for text that was removed in a diff
 */
export const DiffRemoved = Mark.create({
  name: 'diffRemoved',

  addAttributes() {
    return {};
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-diff-removed]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        'data-diff-removed': '',
        class: 'diff-removed',
        style: 'background-color: rgba(239, 68, 68, 0.3); color: inherit; text-decoration: line-through;',
      }),
      0,
    ];
  },
});
