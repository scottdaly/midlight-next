import { Mark, mergeAttributes } from '@tiptap/core';

export interface AIAnnotationAttributes {
  conversationId: string;
  messageId: string;
  type: 'edit' | 'suggestion' | 'reference';
  tooltip?: string;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    aiAnnotation: {
      /**
       * Set an AI annotation mark
       */
      setAIAnnotation: (attributes: AIAnnotationAttributes) => ReturnType;
      /**
       * Toggle an AI annotation mark
       */
      toggleAIAnnotation: (attributes: AIAnnotationAttributes) => ReturnType;
      /**
       * Unset an AI annotation mark
       */
      unsetAIAnnotation: () => ReturnType;
      /**
       * Remove all AI annotations
       */
      removeAllAIAnnotations: () => ReturnType;
    };
  }
}

/**
 * Mark for AI-generated content annotations
 * Used to show where AI made changes in the document
 */
export const AIAnnotation = Mark.create<{ HTMLAttributes: Record<string, unknown> }>({
  name: 'aiAnnotation',

  addOptions() {
    return {
      HTMLAttributes: {},
    };
  },

  addAttributes() {
    return {
      conversationId: {
        default: null,
        parseHTML: (element) => element.getAttribute('data-conversation-id'),
        renderHTML: (attributes) => {
          if (!attributes.conversationId) return {};
          return { 'data-conversation-id': attributes.conversationId };
        },
      },
      messageId: {
        default: null,
        parseHTML: (element) => element.getAttribute('data-message-id'),
        renderHTML: (attributes) => {
          if (!attributes.messageId) return {};
          return { 'data-message-id': attributes.messageId };
        },
      },
      type: {
        default: 'edit',
        parseHTML: (element) => element.getAttribute('data-annotation-type') || 'edit',
        renderHTML: (attributes) => {
          return { 'data-annotation-type': attributes.type };
        },
      },
      tooltip: {
        default: null,
        parseHTML: (element) => element.getAttribute('data-tooltip'),
        renderHTML: (attributes) => {
          if (!attributes.tooltip) return {};
          return { 'data-tooltip': attributes.tooltip };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-ai-annotation]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    const type = HTMLAttributes['data-annotation-type'] || 'edit';

    // Different subtle colors based on annotation type
    const colors: Record<string, string> = {
      edit: 'rgba(59, 130, 246, 0.15)',      // blue
      suggestion: 'rgba(245, 158, 11, 0.15)', // amber
      reference: 'rgba(139, 92, 246, 0.15)',  // purple
    };

    const borderColors: Record<string, string> = {
      edit: 'rgba(59, 130, 246, 0.4)',
      suggestion: 'rgba(245, 158, 11, 0.4)',
      reference: 'rgba(139, 92, 246, 0.4)',
    };

    return [
      'span',
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
        'data-ai-annotation': '',
        class: 'ai-annotation',
        style: `
          background-color: ${colors[type] || colors.edit};
          border-bottom: 2px solid ${borderColors[type] || borderColors.edit};
          cursor: pointer;
        `.replace(/\s+/g, ' ').trim(),
      }),
      0,
    ];
  },

  addCommands() {
    return {
      setAIAnnotation:
        (attributes) =>
        ({ commands }) => {
          return commands.setMark(this.name, attributes);
        },
      toggleAIAnnotation:
        (attributes) =>
        ({ commands }) => {
          return commands.toggleMark(this.name, attributes);
        },
      unsetAIAnnotation:
        () =>
        ({ commands }) => {
          return commands.unsetMark(this.name);
        },
      removeAllAIAnnotations:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) {
            const { doc } = tr;
            doc.descendants((node, pos) => {
              if (node.isText) {
                const marks = node.marks.filter((mark) => mark.type.name === this.name);
                marks.forEach((mark) => {
                  tr.removeMark(pos, pos + node.nodeSize, mark.type);
                });
              }
            });
          }
          return true;
        },
    };
  },
});
