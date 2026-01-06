import Code from '@tiptap/extension-code';

/**
 * Custom Code extension without input rules
 * This prevents the automatic insertion of backticks as literal text
 */
export const CustomCode = Code.extend({
  addInputRules() {
    return [];
  },

  addPasteRules() {
    return [];
  },
});
