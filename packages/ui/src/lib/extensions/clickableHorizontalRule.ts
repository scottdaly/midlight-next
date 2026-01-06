import HorizontalRule from '@tiptap/extension-horizontal-rule';

export const ClickableHorizontalRule = HorizontalRule.extend({
  addNodeView() {
    return ({ HTMLAttributes, getPos, editor }) => {
      const dom = document.createElement('div');
      dom.className = 'hr-wrapper';
      dom.setAttribute('data-type', 'horizontal-rule');

      const hr = document.createElement('hr');

      Object.entries(HTMLAttributes).forEach(([key, value]) => {
        if (value !== undefined && value !== null) {
          hr.setAttribute(key, String(value));
        }
      });

      dom.appendChild(hr);

      const handleClick = (event: MouseEvent) => {
        event.preventDefault();
        event.stopPropagation();

        if (typeof getPos === 'function') {
          const pos = getPos();
          if (pos !== undefined) {
            editor.commands.setNodeSelection(pos);
            editor.commands.focus();
          }
        }
      };

      dom.addEventListener('mousedown', handleClick);

      return {
        dom,
        destroy() {
          dom.removeEventListener('mousedown', handleClick);
        },
      };
    };
  },
});
