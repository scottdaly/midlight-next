/**
 * Text color extension for Tiptap
 * Adds color attribute to textStyle mark, allowing foreground text color changes
 * Colors are stored as hex values (e.g., #ff0000) in inline styles
 */

import { Extension } from '@tiptap/core';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    textColor: {
      setTextColor: (color: string) => ReturnType;
      unsetTextColor: () => ReturnType;
    };
  }
}

export const TextColor = Extension.create({
  name: 'textColor',

  addOptions() {
    return {
      types: ['textStyle'],
    };
  },

  addGlobalAttributes() {
    return [
      {
        types: this.options.types,
        attributes: {
          color: {
            default: null,
            parseHTML: (element: HTMLElement) => element.style.color || null,
            renderHTML: (attributes: Record<string, string>) => {
              if (!attributes.color) {
                return {};
              }
              return {
                style: `color: ${attributes.color}`,
              };
            },
          },
        },
      },
    ];
  },

  addCommands() {
    return {
      setTextColor: (color: string) => ({ chain }) => {
        return chain()
          .setMark('textStyle', { color })
          .run();
      },
      unsetTextColor: () => ({ chain }) => {
        return chain()
          .setMark('textStyle', { color: null })
          .removeEmptyTextStyle()
          .run();
      },
    };
  },
});
