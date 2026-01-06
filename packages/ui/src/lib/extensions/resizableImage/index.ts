import Image from '@tiptap/extension-image';

export interface ResizableImageAttributes {
  src: string | null;
  alt: string | null;
  title: string | null;
  width: string;
  height: string;
  align: 'left-wrap' | 'left-break' | 'center-break' | 'right-break' | 'right-wrap';
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    resizableImage: {
      setImageAlign: (align: ResizableImageAttributes['align']) => ReturnType;
      setImageWidth: (width: string) => ReturnType;
    };
  }
}

export const ResizableImage = Image.extend({
  name: 'resizableImage',

  addAttributes() {
    return {
      ...this.parent?.(),
      src: {
        default: null,
        renderHTML: (attributes) => ({
          src: attributes.src,
        }),
        parseHTML: (element) => element.getAttribute('src'),
      },
      width: {
        default: '100%',
        renderHTML: (attributes) => ({
          width: attributes.width,
        }),
        parseHTML: (element) => element.getAttribute('width') || '100%',
      },
      height: {
        default: 'auto',
        renderHTML: (attributes) => ({
          height: attributes.height,
        }),
        parseHTML: (element) => element.getAttribute('height') || 'auto',
      },
      align: {
        default: 'center-break',
        renderHTML: (attributes) => ({
          'data-align': attributes.align,
        }),
        parseHTML: (element) => element.getAttribute('data-align') || 'center-break',
      },
    };
  },

  addCommands() {
    return {
      ...this.parent?.(),
      setImageAlign:
        (align) =>
        ({ commands }) => {
          return commands.updateAttributes('resizableImage', { align });
        },
      setImageWidth:
        (width) =>
        ({ commands }) => {
          return commands.updateAttributes('resizableImage', { width });
        },
    };
  },

  addNodeView() {
    return ({ node, getPos, editor, HTMLAttributes }) => {
      // Alignment styles
      const alignmentStyles: Record<string, string> = {
        'left-wrap': 'float: left; margin-right: 1rem; margin-bottom: 1rem;',
        'left-break': 'display: flex; justify-content: flex-start; width: 100%;',
        'center-break': 'display: flex; justify-content: center; width: 100%;',
        'right-break': 'display: flex; justify-content: flex-end; width: 100%;',
        'right-wrap': 'float: right; margin-left: 1rem; margin-bottom: 1rem;',
      };

      // Create wrapper
      const wrapper = document.createElement('div');
      wrapper.className = 'resizable-image-wrapper';
      wrapper.style.cssText = `position: relative; ${alignmentStyles[node.attrs.align] || alignmentStyles['center-break']}`;

      // Create container for image and handles
      const container = document.createElement('div');
      container.className = 'resizable-image-container';
      container.style.cssText = 'position: relative; display: inline-block;';

      // Create image
      const img = document.createElement('img');
      img.src = node.attrs.src || '';
      img.alt = node.attrs.alt || '';
      img.style.cssText = `width: ${node.attrs.width}; height: ${node.attrs.height}; max-width: 100%; display: block; border-radius: 0.125rem;`;

      Object.entries(HTMLAttributes).forEach(([key, value]) => {
        if (value !== undefined && value !== null && key !== 'style') {
          img.setAttribute(key, String(value));
        }
      });

      container.appendChild(img);

      // Create resize handles
      const handlePositions = [
        { pos: 'nw', cursor: 'nwse-resize', style: 'top: -6px; left: -6px;', dir: 'left' },
        { pos: 'ne', cursor: 'nesw-resize', style: 'top: -6px; right: -6px;', dir: 'right' },
        { pos: 'sw', cursor: 'nesw-resize', style: 'bottom: -6px; left: -6px;', dir: 'left' },
        { pos: 'se', cursor: 'nwse-resize', style: 'bottom: -6px; right: -6px;', dir: 'right' },
        { pos: 'w', cursor: 'ew-resize', style: 'top: 50%; transform: translateY(-50%); left: -6px;', dir: 'left' },
        { pos: 'e', cursor: 'ew-resize', style: 'top: 50%; transform: translateY(-50%); right: -6px;', dir: 'right' },
      ];

      const handles: HTMLElement[] = [];

      handlePositions.forEach(({ pos, cursor, style, dir }) => {
        const handle = document.createElement('div');
        handle.className = `resize-handle resize-handle-${pos}`;
        handle.style.cssText = `
          position: absolute;
          width: 12px;
          height: 12px;
          background: var(--color-primary, #3b82f6);
          border-radius: 50%;
          cursor: ${cursor};
          z-index: 50;
          opacity: 0;
          transition: opacity 0.15s, transform 0.15s;
          ${style}
        `;
        handle.dataset.direction = dir;
        handles.push(handle);
        container.appendChild(handle);
      });

      wrapper.appendChild(container);

      // State
      let isSelected = false;
      let isResizing = false;

      // Update selection state
      const updateSelection = (selected: boolean) => {
        isSelected = selected;
        container.style.outline = selected ? '2px solid var(--color-primary, #3b82f6)' : 'none';
        container.style.outlineOffset = '2px';
        handles.forEach(h => {
          h.style.opacity = selected && !isResizing ? '1' : '0';
        });
      };

      // Resize handling
      const handleMouseDown = (e: MouseEvent) => {
        e.preventDefault();
        e.stopPropagation();

        const handle = e.target as HTMLElement;
        const direction = handle.dataset.direction;
        if (!direction) return;

        isResizing = true;
        handles.forEach(h => h.style.opacity = '1');

        const startX = e.clientX;
        const startWidth = img.offsetWidth;

        const onMouseMove = (moveEvent: MouseEvent) => {
          moveEvent.preventDefault();
          const diff = moveEvent.clientX - startX;
          const newWidth = direction === 'left'
            ? Math.max(50, startWidth - diff)
            : Math.max(50, startWidth + diff);
          img.style.width = `${newWidth}px`;
        };

        const onMouseUp = () => {
          isResizing = false;
          document.removeEventListener('mousemove', onMouseMove);
          document.removeEventListener('mouseup', onMouseUp);

          // Persist width to node
          if (typeof getPos === 'function') {
            const pos = getPos();
            if (pos !== undefined) {
              editor.commands.command(({ tr }) => {
                tr.setNodeMarkup(pos, undefined, {
                  ...node.attrs,
                  width: `${img.offsetWidth}px`,
                });
                return true;
              });
            }
          }

          updateSelection(isSelected);
        };

        document.addEventListener('mousemove', onMouseMove);
        document.addEventListener('mouseup', onMouseUp);
      };

      // Add event listeners
      handles.forEach(handle => {
        handle.addEventListener('mousedown', handleMouseDown);
      });

      // Click to select
      container.addEventListener('click', (e) => {
        e.preventDefault();
        if (typeof getPos === 'function') {
          const pos = getPos();
          if (pos !== undefined) {
            editor.commands.setNodeSelection(pos);
          }
        }
      });

      // Hover effect
      container.addEventListener('mouseenter', () => {
        if (!isResizing) {
          handles.forEach(h => h.style.opacity = '0.5');
        }
      });

      container.addEventListener('mouseleave', () => {
        if (!isResizing && !isSelected) {
          handles.forEach(h => h.style.opacity = '0');
        }
      });

      return {
        dom: wrapper,
        contentDOM: null,
        update: (updatedNode) => {
          if (updatedNode.type.name !== 'resizableImage') return false;

          img.src = updatedNode.attrs.src || '';
          img.alt = updatedNode.attrs.alt || '';
          img.style.width = updatedNode.attrs.width;
          img.style.height = updatedNode.attrs.height;
          wrapper.style.cssText = `position: relative; ${alignmentStyles[updatedNode.attrs.align] || alignmentStyles['center-break']}`;

          return true;
        },
        selectNode: () => {
          updateSelection(true);
        },
        deselectNode: () => {
          updateSelection(false);
        },
        destroy: () => {
          handles.forEach(handle => {
            handle.removeEventListener('mousedown', handleMouseDown);
          });
        },
      };
    };
  },
});

export default ResizableImage;
