// @midlight/core/serialization/documentSerializer
// Converts Tiptap JSON to Markdown + Sidecar

import type {
  TiptapDocument,
  TiptapNode,
  SidecarDocument,
  BlockFormatting,
  SpanFormatting,
  ImageInfo,
} from '../types/index.js';
import { createEmptySidecar, countWords, estimateReadingTime } from './index.js';
import { generateBlockId } from '../utils/index.js';

export interface SerializeResult {
  markdown: string;
  sidecar: SidecarDocument;
}

export interface SerializeOptions {
  /**
   * Function to store images and return a reference
   * If not provided, images are kept as data URLs
   */
  storeImage?: (dataUrl: string, name?: string) => Promise<string>;
}

/**
 * Serializes a Tiptap document to Markdown + Sidecar format
 *
 * The markdown output is clean and readable, while the sidecar
 * preserves all rich formatting that can't be represented in markdown.
 */
export class DocumentSerializer {
  private options: SerializeOptions;
  private sidecar: SidecarDocument;
  private imageRefs: Map<string, string> = new Map();

  constructor(options: SerializeOptions = {}) {
    this.options = options;
    this.sidecar = createEmptySidecar();
  }

  async serialize(doc: TiptapDocument): Promise<SerializeResult> {
    this.sidecar = createEmptySidecar();
    this.imageRefs.clear();

    const lines: string[] = [];

    for (const node of doc.content || []) {
      const result = await this.serializeNode(node);
      if (result !== null) {
        lines.push(result);
      }
    }

    const markdown = lines.join('\n\n');

    // Update metadata
    const wordCount = countWords(markdown);
    this.sidecar.meta.wordCount = wordCount;
    this.sidecar.meta.readingTime = estimateReadingTime(wordCount);
    this.sidecar.meta.modified = new Date().toISOString();

    return {
      markdown,
      sidecar: this.sidecar,
    };
  }

  private async serializeNode(node: TiptapNode): Promise<string | null> {
    const blockId = (node.attrs?.blockId as string) || generateBlockId();

    switch (node.type) {
      case 'paragraph':
        return this.serializeParagraph(node, blockId);

      case 'heading':
        return this.serializeHeading(node, blockId);

      case 'bulletList':
        return this.serializeList(node, '-');

      case 'orderedList':
        return this.serializeList(node, '1.');

      case 'blockquote':
        return this.serializeBlockquote(node, blockId);

      case 'codeBlock':
        return this.serializeCodeBlock(node, blockId);

      case 'horizontalRule':
        return '---';

      case 'image':
        return this.serializeImage(node, blockId);

      case 'table':
        return this.serializeTable(node);

      default:
        // Unknown node type - try to extract text
        return this.extractText(node);
    }
  }

  private serializeParagraph(node: TiptapNode, blockId: string): string {
    const text = this.serializeInlineContent(node.content || [], blockId);
    this.captureBlockFormatting(node, blockId);
    return `<!-- @mid:${blockId} -->\n${text}`;
  }

  private serializeHeading(node: TiptapNode, blockId: string): string {
    const level = (node.attrs?.level as number) || 1;
    const prefix = '#'.repeat(level);
    const text = this.serializeInlineContent(node.content || [], blockId);
    this.captureBlockFormatting(node, blockId);
    return `<!-- @mid:${blockId} -->\n${prefix} ${text}`;
  }

  private serializeList(node: TiptapNode, marker: string): string {
    const items: string[] = [];

    for (const item of node.content || []) {
      if (item.type === 'listItem') {
        const content = this.serializeListItem(item, marker);
        items.push(content);
      }
    }

    return items.join('\n');
  }

  private serializeListItem(node: TiptapNode, marker: string): string {
    const parts: string[] = [];

    for (const child of node.content || []) {
      if (child.type === 'paragraph') {
        const text = this.serializeInlineContent(child.content || [], '');
        parts.push(`${marker} ${text}`);
      } else if (child.type === 'bulletList' || child.type === 'orderedList') {
        // Nested list - indent
        const nested = this.serializeList(child, marker === '-' ? '-' : '1.');
        const indented = nested
          .split('\n')
          .map((line) => '  ' + line)
          .join('\n');
        parts.push(indented);
      }
    }

    return parts.join('\n');
  }

  private serializeBlockquote(node: TiptapNode, blockId: string): string {
    const content: string[] = [];

    for (const child of node.content || []) {
      const text = this.extractText(child);
      content.push(`> ${text}`);
    }

    this.captureBlockFormatting(node, blockId);
    return `<!-- @mid:${blockId} -->\n${content.join('\n')}`;
  }

  private serializeCodeBlock(node: TiptapNode, blockId: string): string {
    const language = (node.attrs?.language as string) || '';
    const code = this.extractText(node);
    return `\`\`\`${language}\n${code}\n\`\`\``;
  }

  private async serializeImage(node: TiptapNode, blockId: string): Promise<string> {
    const src = node.attrs?.src as string;
    const alt = (node.attrs?.alt as string) || '';
    const title = node.attrs?.title as string | undefined;

    let imageRef = src;

    // If it's a data URL and we have a store function, deduplicate
    if (src?.startsWith('data:') && this.options.storeImage) {
      if (!this.imageRefs.has(src)) {
        const ref = await this.options.storeImage(src);
        this.imageRefs.set(src, ref);
      }
      imageRef = this.imageRefs.get(src)!;
    }

    // Store image info in sidecar
    this.sidecar.images[blockId] = {
      ref: imageRef,
      alt,
      title,
      width: node.attrs?.width as number | undefined,
      height: node.attrs?.height as number | undefined,
      alignment: node.attrs?.alignment as 'left' | 'center' | 'right' | undefined,
    };

    const titlePart = title ? ` "${title}"` : '';
    return `![${alt}](${imageRef}${titlePart})`;
  }

  private serializeTable(node: TiptapNode): string {
    // TODO: Implement table serialization
    return '<!-- table placeholder -->';
  }

  private serializeInlineContent(nodes: TiptapNode[], blockId: string): string {
    let result = '';
    const spans: SpanFormatting[] = [];
    let position = 0;

    for (const node of nodes) {
      if (node.type === 'text' && node.text) {
        const text = node.text;
        const marks = node.marks || [];

        // Apply markdown marks
        let decorated = text;
        for (const mark of marks) {
          switch (mark.type) {
            case 'bold':
              decorated = `**${decorated}**`;
              break;
            case 'italic':
              decorated = `*${decorated}*`;
              break;
            case 'code':
              decorated = `\`${decorated}\``;
              break;
            case 'link':
              decorated = `[${decorated}](${mark.attrs?.href || ''})`;
              break;
            case 'strike':
              decorated = `~~${decorated}~~`;
              break;
          }
        }

        // Capture non-markdown formatting in sidecar
        const spanFormatting = this.extractSpanFormatting(marks);
        if (Object.keys(spanFormatting).length > 0) {
          spans.push({
            start: position,
            end: position + text.length,
            ...spanFormatting,
          });
        }

        result += decorated;
        position += text.length;
      }
    }

    if (spans.length > 0 && blockId) {
      this.sidecar.spans[blockId] = spans;
    }

    return result;
  }

  private extractSpanFormatting(marks: TiptapNode['marks']): Partial<SpanFormatting> {
    const formatting: Partial<SpanFormatting> = {};

    for (const mark of marks || []) {
      switch (mark.type) {
        case 'textStyle':
          if (mark.attrs?.fontFamily) formatting.fontFamily = mark.attrs.fontFamily as string;
          if (mark.attrs?.fontSize) formatting.fontSize = mark.attrs.fontSize as string;
          if (mark.attrs?.color) formatting.color = mark.attrs.color as string;
          break;
        case 'highlight':
          formatting.backgroundColor = mark.attrs?.color as string;
          break;
        case 'underline':
          formatting.underline = true;
          break;
        case 'superscript':
          formatting.superscript = true;
          break;
        case 'subscript':
          formatting.subscript = true;
          break;
      }
    }

    return formatting;
  }

  private captureBlockFormatting(node: TiptapNode, blockId: string): void {
    const attrs = node.attrs || {};
    const formatting: BlockFormatting = {};

    if (attrs.textAlign && attrs.textAlign !== 'left') {
      formatting.textAlign = attrs.textAlign as BlockFormatting['textAlign'];
    }
    if (attrs.indent) {
      formatting.indent = attrs.indent as number;
    }

    if (Object.keys(formatting).length > 0) {
      this.sidecar.blocks[blockId] = formatting;
    }
  }

  private extractText(node: TiptapNode): string {
    if (node.text) return node.text;

    const parts: string[] = [];
    for (const child of node.content || []) {
      parts.push(this.extractText(child));
    }
    return parts.join('');
  }
}
