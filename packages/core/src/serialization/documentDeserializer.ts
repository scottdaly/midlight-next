// @midlight/core/serialization/documentDeserializer
// Converts Markdown + Sidecar back to Tiptap JSON

import type {
  TiptapDocument,
  TiptapNode,
  TiptapMark,
  SidecarDocument,
  BlockFormatting,
  SpanFormatting,
} from '../types/index.js';
import { createEmptySidecar } from './index.js';

export interface DeserializeOptions {
  /**
   * Function to load image data from a reference
   * Returns a data URL or throws if not found
   */
  loadImage?: (ref: string) => Promise<string>;
}

/**
 * Deserializes Markdown + Sidecar back to Tiptap JSON document
 *
 * Parses markdown and reapplies formatting from the sidecar to
 * reconstruct the full rich text document.
 */
export class DocumentDeserializer {
  private options: DeserializeOptions;
  private sidecar: SidecarDocument;

  constructor(options: DeserializeOptions = {}) {
    this.options = options;
    this.sidecar = createEmptySidecar();
  }

  async deserialize(markdown: string, sidecar?: SidecarDocument): Promise<TiptapDocument> {
    this.sidecar = sidecar || createEmptySidecar();

    const lines = markdown.split('\n');
    const nodes: TiptapNode[] = [];
    let i = 0;

    while (i < lines.length) {
      const result = this.parseBlock(lines, i);
      if (result.node) {
        nodes.push(result.node);
      }
      i = result.nextIndex;
    }

    return {
      type: 'doc',
      content: nodes,
    };
  }

  private parseBlock(lines: string[], index: number): { node: TiptapNode | null; nextIndex: number } {
    const line = lines[index];

    // Skip empty lines
    if (!line || line.trim() === '') {
      return { node: null, nextIndex: index + 1 };
    }

    // Check for block ID comment
    let blockId: string | undefined;
    let contentLine = line;

    const blockIdMatch = line.match(/^<!-- @mid:([a-zA-Z0-9_-]+) -->$/);
    if (blockIdMatch) {
      blockId = blockIdMatch[1];
      index++;
      contentLine = lines[index] || '';
    }

    // Parse different block types
    if (contentLine.startsWith('# ')) {
      return this.parseHeading(contentLine, 1, blockId, index);
    }
    if (contentLine.startsWith('## ')) {
      return this.parseHeading(contentLine, 2, blockId, index);
    }
    if (contentLine.startsWith('### ')) {
      return this.parseHeading(contentLine, 3, blockId, index);
    }
    if (contentLine.startsWith('#### ')) {
      return this.parseHeading(contentLine, 4, blockId, index);
    }
    if (contentLine.startsWith('##### ')) {
      return this.parseHeading(contentLine, 5, blockId, index);
    }
    if (contentLine.startsWith('###### ')) {
      return this.parseHeading(contentLine, 6, blockId, index);
    }

    if (contentLine.startsWith('- ') || contentLine.startsWith('* ')) {
      return this.parseBulletList(lines, index);
    }

    if (/^\d+\.\s/.test(contentLine)) {
      return this.parseOrderedList(lines, index);
    }

    if (contentLine.startsWith('> ')) {
      return this.parseBlockquote(lines, index, blockId);
    }

    if (contentLine.startsWith('```')) {
      return this.parseCodeBlock(lines, index);
    }

    if (contentLine === '---' || contentLine === '***' || contentLine === '___') {
      return {
        node: { type: 'horizontalRule' },
        nextIndex: index + 1,
      };
    }

    if (contentLine.startsWith('![')) {
      return this.parseImage(contentLine, blockId, index);
    }

    // Default: paragraph
    return this.parseParagraph(contentLine, blockId, index);
  }

  private parseHeading(
    line: string,
    level: number,
    blockId: string | undefined,
    index: number
  ): { node: TiptapNode; nextIndex: number } {
    const prefix = '#'.repeat(level) + ' ';
    const text = line.slice(prefix.length);
    const content = this.parseInlineContent(text, blockId);

    const attrs: Record<string, unknown> = { level };
    if (blockId) attrs.blockId = blockId;

    // Apply block formatting from sidecar
    if (blockId && this.sidecar.blocks[blockId]) {
      const formatting = this.sidecar.blocks[blockId];
      if (formatting.textAlign) attrs.textAlign = formatting.textAlign;
    }

    return {
      node: {
        type: 'heading',
        attrs,
        content,
      },
      nextIndex: index + 1,
    };
  }

  private parseParagraph(
    line: string,
    blockId: string | undefined,
    index: number
  ): { node: TiptapNode; nextIndex: number } {
    const content = this.parseInlineContent(line, blockId);

    const attrs: Record<string, unknown> = {};
    if (blockId) attrs.blockId = blockId;

    // Apply block formatting from sidecar
    if (blockId && this.sidecar.blocks[blockId]) {
      const formatting = this.sidecar.blocks[blockId];
      if (formatting.textAlign) attrs.textAlign = formatting.textAlign;
      if (formatting.indent) attrs.indent = formatting.indent;
    }

    return {
      node: {
        type: 'paragraph',
        attrs: Object.keys(attrs).length > 0 ? attrs : undefined,
        content,
      },
      nextIndex: index + 1,
    };
  }

  private parseBulletList(
    lines: string[],
    startIndex: number
  ): { node: TiptapNode; nextIndex: number } {
    const items: TiptapNode[] = [];
    let i = startIndex;

    while (i < lines.length) {
      const line = lines[i];
      if (!line.match(/^[-*]\s/)) break;

      const text = line.slice(2);
      items.push({
        type: 'listItem',
        content: [
          {
            type: 'paragraph',
            content: this.parseInlineContent(text),
          },
        ],
      });
      i++;
    }

    return {
      node: {
        type: 'bulletList',
        content: items,
      },
      nextIndex: i,
    };
  }

  private parseOrderedList(
    lines: string[],
    startIndex: number
  ): { node: TiptapNode; nextIndex: number } {
    const items: TiptapNode[] = [];
    let i = startIndex;

    while (i < lines.length) {
      const line = lines[i];
      const match = line.match(/^\d+\.\s(.*)$/);
      if (!match) break;

      items.push({
        type: 'listItem',
        content: [
          {
            type: 'paragraph',
            content: this.parseInlineContent(match[1]),
          },
        ],
      });
      i++;
    }

    return {
      node: {
        type: 'orderedList',
        content: items,
      },
      nextIndex: i,
    };
  }

  private parseBlockquote(
    lines: string[],
    startIndex: number,
    blockId: string | undefined
  ): { node: TiptapNode; nextIndex: number } {
    const content: TiptapNode[] = [];
    let i = startIndex;

    while (i < lines.length && lines[i].startsWith('> ')) {
      const text = lines[i].slice(2);
      content.push({
        type: 'paragraph',
        content: this.parseInlineContent(text),
      });
      i++;
    }

    return {
      node: {
        type: 'blockquote',
        attrs: blockId ? { blockId } : undefined,
        content,
      },
      nextIndex: i,
    };
  }

  private parseCodeBlock(
    lines: string[],
    startIndex: number
  ): { node: TiptapNode; nextIndex: number } {
    const firstLine = lines[startIndex];
    const language = firstLine.slice(3).trim();
    const codeLines: string[] = [];
    let i = startIndex + 1;

    while (i < lines.length && !lines[i].startsWith('```')) {
      codeLines.push(lines[i]);
      i++;
    }

    return {
      node: {
        type: 'codeBlock',
        attrs: language ? { language } : undefined,
        content: [{ type: 'text', text: codeLines.join('\n') }],
      },
      nextIndex: i + 1, // Skip closing ```
    };
  }

  private parseImage(
    line: string,
    blockId: string | undefined,
    index: number
  ): { node: TiptapNode; nextIndex: number } {
    // Parse ![alt](src "title")
    const match = line.match(/^!\[([^\]]*)\]\(([^)"]+)(?:\s+"([^"]+)")?\)/);
    if (!match) {
      return this.parseParagraph(line, blockId, index);
    }

    const [, alt, src, title] = match;

    const attrs: Record<string, unknown> = {
      src,
      alt,
    };
    if (title) attrs.title = title;
    if (blockId) attrs.blockId = blockId;

    // Apply image formatting from sidecar
    if (blockId && this.sidecar.images[blockId]) {
      const imageInfo = this.sidecar.images[blockId];
      if (imageInfo.width) attrs.width = imageInfo.width;
      if (imageInfo.height) attrs.height = imageInfo.height;
      if (imageInfo.alignment) attrs.alignment = imageInfo.alignment;
    }

    return {
      node: {
        type: 'image',
        attrs,
      },
      nextIndex: index + 1,
    };
  }

  private parseInlineContent(text: string, blockId?: string): TiptapNode[] {
    const nodes: TiptapNode[] = [];
    let remaining = text;
    let position = 0;

    // Get span formatting from sidecar
    const spans = blockId ? this.sidecar.spans[blockId] || [] : [];

    while (remaining.length > 0) {
      // Check for bold **text**
      const boldMatch = remaining.match(/^\*\*([^*]+)\*\*/);
      if (boldMatch) {
        nodes.push(this.createTextNode(boldMatch[1], [{ type: 'bold' }], position, spans));
        position += boldMatch[1].length;
        remaining = remaining.slice(boldMatch[0].length);
        continue;
      }

      // Check for italic *text*
      const italicMatch = remaining.match(/^\*([^*]+)\*/);
      if (italicMatch) {
        nodes.push(this.createTextNode(italicMatch[1], [{ type: 'italic' }], position, spans));
        position += italicMatch[1].length;
        remaining = remaining.slice(italicMatch[0].length);
        continue;
      }

      // Check for code `text`
      const codeMatch = remaining.match(/^`([^`]+)`/);
      if (codeMatch) {
        nodes.push(this.createTextNode(codeMatch[1], [{ type: 'code' }], position, spans));
        position += codeMatch[1].length;
        remaining = remaining.slice(codeMatch[0].length);
        continue;
      }

      // Check for link [text](url)
      const linkMatch = remaining.match(/^\[([^\]]+)\]\(([^)]+)\)/);
      if (linkMatch) {
        nodes.push(
          this.createTextNode(
            linkMatch[1],
            [{ type: 'link', attrs: { href: linkMatch[2] } }],
            position,
            spans
          )
        );
        position += linkMatch[1].length;
        remaining = remaining.slice(linkMatch[0].length);
        continue;
      }

      // Check for strikethrough ~~text~~
      const strikeMatch = remaining.match(/^~~([^~]+)~~/);
      if (strikeMatch) {
        nodes.push(this.createTextNode(strikeMatch[1], [{ type: 'strike' }], position, spans));
        position += strikeMatch[1].length;
        remaining = remaining.slice(strikeMatch[0].length);
        continue;
      }

      // Plain text - find next special character
      const nextSpecial = remaining.search(/[*`\[~]/);
      if (nextSpecial === -1) {
        // Rest is plain text
        nodes.push(this.createTextNode(remaining, [], position, spans));
        break;
      } else if (nextSpecial === 0) {
        // Special char at start but didn't match pattern - treat as text
        nodes.push(this.createTextNode(remaining[0], [], position, spans));
        position += 1;
        remaining = remaining.slice(1);
      } else {
        // Plain text before special char
        const plainText = remaining.slice(0, nextSpecial);
        nodes.push(this.createTextNode(plainText, [], position, spans));
        position += plainText.length;
        remaining = remaining.slice(nextSpecial);
      }
    }

    return nodes;
  }

  private createTextNode(
    text: string,
    marks: TiptapMark[],
    position: number,
    spans: SpanFormatting[]
  ): TiptapNode {
    // Find applicable span formatting
    const applicableSpans = spans.filter(
      (s) => s.start <= position && s.end >= position + text.length
    );

    // Apply span formatting as marks
    for (const span of applicableSpans) {
      if (span.fontFamily || span.fontSize || span.color) {
        const textStyleAttrs: Record<string, string> = {};
        if (span.fontFamily) textStyleAttrs.fontFamily = span.fontFamily;
        if (span.fontSize) textStyleAttrs.fontSize = span.fontSize;
        if (span.color) textStyleAttrs.color = span.color;
        marks.push({ type: 'textStyle', attrs: textStyleAttrs });
      }
      if (span.backgroundColor) {
        marks.push({ type: 'highlight', attrs: { color: span.backgroundColor } });
      }
      if (span.underline) {
        marks.push({ type: 'underline' });
      }
      if (span.superscript) {
        marks.push({ type: 'superscript' });
      }
      if (span.subscript) {
        marks.push({ type: 'subscript' });
      }
    }

    return {
      type: 'text',
      text,
      marks: marks.length > 0 ? marks : undefined,
    };
  }
}
