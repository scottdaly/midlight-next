// Midlight Tiptap Extensions
// Re-exports all custom extensions for the editor

// Text styling
export { FontSize } from './fontSize';
export { TextColor } from './textColor';
export { TextHighlight } from './textHighlight';

// Code
export { CustomCode } from './customCode';

// Diff marks for showing changes
export { DiffAdded, DiffRemoved } from './diffMark';

// AI annotations
export { AIAnnotation } from './aiAnnotation';
export type { AIAnnotationAttributes } from './aiAnnotation';

// Node extensions
export { ClickableHorizontalRule } from './clickableHorizontalRule';
export { ResizableImage } from './resizableImage';
export type { ResizableImageAttributes } from './resizableImage';

// Page layout
export { PageSplitting, PAGE_BREAKS_UPDATED_EVENT, pageSplittingPluginKey } from './pageSplitting';
export type { PageBreak, PageSplittingStorage } from './pageSplitting';
export { PAGE_WIDTH, PAGE_HEIGHT, PAGE_PADDING, CONTENT_HEIGHT, PAGE_GAP } from './pageSplitting';
