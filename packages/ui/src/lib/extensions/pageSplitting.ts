import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { EditorView } from '@tiptap/pm/view';

// Page dimensions (8.5" x 11" at 96 DPI)
export const PAGE_WIDTH = 816;
export const PAGE_HEIGHT = 1056;
export const PAGE_PADDING = 48;
export const CONTENT_HEIGHT = PAGE_HEIGHT - (PAGE_PADDING * 2); // 960px
export const PAGE_GAP = 32;

export interface PageBreak {
  pageNumber: number;
  startPos: number;      // Document position where page starts
  endPos: number;        // Document position where page ends
  topOffset: number;     // Pixel offset from editor top
  height: number;        // Height of content on this page
}

export interface PageSplittingStorage {
  pageBreaks: PageBreak[];
  totalHeight: number;
  isCalculating: boolean;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    pageSplitting: {
      recalculatePageBreaks: () => ReturnType;
    };
  }
}

export const pageSplittingPluginKey = new PluginKey('pageSplitting');

// Custom event for page break updates
export const PAGE_BREAKS_UPDATED_EVENT = 'page-breaks-updated';

export const PageSplitting = Extension.create<object, PageSplittingStorage>({
  name: 'pageSplitting',

  addStorage() {
    return {
      pageBreaks: [],
      totalHeight: 0,
      isCalculating: false,
    };
  },

  addCommands() {
    return {
      recalculatePageBreaks:
        () =>
        ({ editor }) => {
          const view = editor.view;
          if (view) {
            calculatePageBreaks(view, this.storage);
          }
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    const storage = this.storage;
    let resizeObserver: ResizeObserver | null = null;
    let debounceTimeout: ReturnType<typeof setTimeout> | null = null;
    let rafId: number | null = null;
    let lastHeight = 0;
    let lastPageBreaksJson = '';

    // Debounced calculation - waits for typing to pause
    const scheduleCalculation = (view: EditorView, immediate = false) => {
      // Clear any pending calculations
      if (debounceTimeout) {
        clearTimeout(debounceTimeout);
        debounceTimeout = null;
      }
      if (rafId) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }

      const doCalculation = () => {
        rafId = requestAnimationFrame(() => {
          const newBreaks = calculatePageBreaks(view, storage);
          // Only emit event if page breaks actually changed
          const newJson = JSON.stringify(newBreaks.map(b => ({ p: b.pageNumber, t: b.topOffset })));
          if (newJson !== lastPageBreaksJson) {
            lastPageBreaksJson = newJson;
            window.dispatchEvent(new CustomEvent(PAGE_BREAKS_UPDATED_EVENT, {
              detail: { pageBreaks: newBreaks, totalHeight: storage.totalHeight },
            }));
          }
          rafId = null;
        });
      };

      if (immediate) {
        doCalculation();
      } else {
        // Debounce: wait 150ms after last change before calculating
        debounceTimeout = setTimeout(doCalculation, 150);
      }
    };

    return [
      new Plugin({
        key: pageSplittingPluginKey,
        view(view) {
          // Set up ResizeObserver to watch for content size changes
          resizeObserver = new ResizeObserver((entries) => {
            for (const entry of entries) {
              const newHeight = entry.contentRect.height;
              // Only recalculate if height changed significantly
              if (Math.abs(newHeight - lastHeight) > 5) {
                lastHeight = newHeight;
                scheduleCalculation(view);
              }
            }
          });

          // Observe the editor's DOM element
          if (view.dom) {
            resizeObserver.observe(view.dom);
          }

          // Initial calculation (immediate)
          scheduleCalculation(view, true);

          return {
            update(view, prevState) {
              // Only recalculate if document actually changed
              if (!view.state.doc.eq(prevState.doc)) {
                scheduleCalculation(view);
              }
            },
            destroy() {
              if (resizeObserver) {
                resizeObserver.disconnect();
                resizeObserver = null;
              }
              if (debounceTimeout) {
                clearTimeout(debounceTimeout);
                debounceTimeout = null;
              }
              if (rafId) {
                cancelAnimationFrame(rafId);
                rafId = null;
              }
            },
          };
        },
      }),
    ];
  },
});

function calculatePageBreaks(view: EditorView, storage: PageSplittingStorage): PageBreak[] {
  if (storage.isCalculating) return storage.pageBreaks;
  storage.isCalculating = true;

  const { doc } = view.state;
  const pageBreaks: PageBreak[] = [];

  let currentPageNumber = 1;
  let currentPageStartPos = 0;
  let currentPageTopOffset = 0;
  let accumulatedHeight = 0;

  // Get the editor's bounding rect for reference
  const editorRect = view.dom.getBoundingClientRect();

  // Walk through top-level nodes
  doc.forEach((_node, offset) => {
    const pos = offset;

    // Get the DOM element for this node
    const domNode = view.nodeDOM(pos);
    if (!domNode || !(domNode instanceof HTMLElement)) {
      return;
    }

    const nodeRect = domNode.getBoundingClientRect();
    const nodeHeight = nodeRect.height;
    const nodeTop = nodeRect.top - editorRect.top;

    // Check if adding this node would overflow the current page
    if (accumulatedHeight + nodeHeight > CONTENT_HEIGHT && accumulatedHeight > 0) {
      // Close the current page
      pageBreaks.push({
        pageNumber: currentPageNumber,
        startPos: currentPageStartPos,
        endPos: pos - 1, // End before this node
        topOffset: currentPageTopOffset,
        height: accumulatedHeight,
      });

      // Start a new page
      currentPageNumber++;
      currentPageStartPos = pos;
      currentPageTopOffset = nodeTop;
      accumulatedHeight = nodeHeight;
    } else {
      accumulatedHeight += nodeHeight;
    }
  });

  // Add the final page
  if (accumulatedHeight > 0 || pageBreaks.length === 0) {
    pageBreaks.push({
      pageNumber: currentPageNumber,
      startPos: currentPageStartPos,
      endPos: doc.content.size,
      topOffset: currentPageTopOffset,
      height: accumulatedHeight,
    });
  }

  // Calculate total height
  const totalHeight = pageBreaks.reduce((sum, page) => sum + page.height, 0);

  // Update storage
  storage.pageBreaks = pageBreaks;
  storage.totalHeight = totalHeight;
  storage.isCalculating = false;

  return pageBreaks;
}

export default PageSplitting;
