// Agent Tool Definitions for Document Operations

import type { ToolDefinition } from '../llm/types';

// ============================================================================
// Tool Result Types
// ============================================================================

export interface ToolResult {
  success: boolean;
  data?: unknown;
  error?: string;
}

export interface ListDocumentsResult {
  files: {
    path: string;
    name: string;
    type: 'file' | 'directory';
    modified?: string;
  }[];
}

export interface ReadDocumentResult {
  path: string;
  content: string;
  title?: string;
}

export interface CreateDocumentResult {
  path: string;
  name: string;
}

export interface EditDocumentResult {
  path: string;
  changeId: string;
  originalContent: string;
  newContent: string;
}

export interface MoveDocumentResult {
  oldPath: string;
  newPath: string;
}

export interface DeleteDocumentResult {
  path: string;
}

export interface SearchDocumentsResult {
  matches: {
    path: string;
    name: string;
    snippet: string;
    line?: number;
  }[];
}

// ============================================================================
// Tool Definitions
// ============================================================================

export const listDocumentsTool: ToolDefinition = {
  name: 'list_documents',
  description:
    'List all documents and folders in a directory. Use this to explore the workspace structure.',
  parameters: {
    type: 'object',
    properties: {
      path: {
        type: 'string',
        description:
          'The directory path to list. Use "/" or empty string for the workspace root.',
      },
    },
    required: [],
  },
};

export const readDocumentTool: ToolDefinition = {
  name: 'read_document',
  description:
    'Read the full content of a document. Use this to understand what a document contains before editing.',
  parameters: {
    type: 'object',
    properties: {
      path: {
        type: 'string',
        description: 'The path to the document to read.',
      },
    },
    required: ['path'],
  },
};

export const createDocumentTool: ToolDefinition = {
  name: 'create_document',
  description:
    'Create a new document with the specified content. The document will be created in the Midlight format.',
  parameters: {
    type: 'object',
    properties: {
      path: {
        type: 'string',
        description:
          'The path where the document should be created. Include the filename (without extension).',
      },
      title: {
        type: 'string',
        description: 'The title of the document.',
      },
      content: {
        type: 'string',
        description: `The initial content of the document in markdown format. Supported formatting:
- Headings: # H1, ## H2, ### H3, etc.
- Bold: **text** or __text__
- Italic: *text* or _text_
- Bold+Italic: ***text***
- Inline code: \`code\`
- Bullet lists: - item or * item
- Numbered lists: 1. item
- Blockquotes: > quote
- Horizontal rules: ---

Use appropriate formatting to make documents visually appealing and well-structured.`,
      },
    },
    required: ['path', 'content'],
  },
};

export const editDocumentTool: ToolDefinition = {
  name: 'edit_document',
  description:
    'Edit an existing document. You can replace the entire content or make targeted changes. Changes are staged for user review.',
  parameters: {
    type: 'object',
    properties: {
      path: {
        type: 'string',
        description: 'The path to the document to edit.',
      },
      content: {
        type: 'string',
        description: `The new content for the document in markdown format. Supported formatting:
- Headings: # H1, ## H2, ### H3, etc.
- Bold: **text** or __text__
- Italic: *text* or _text_
- Bold+Italic: ***text***
- Inline code: \`code\`
- Bullet lists: - item or * item
- Numbered lists: 1. item
- Blockquotes: > quote
- Horizontal rules: ---

Preserve existing formatting when making edits, and use appropriate formatting for new content.`,
      },
      description: {
        type: 'string',
        description: 'A brief description of the changes being made.',
      },
    },
    required: ['path', 'content'],
  },
};

export const moveDocumentTool: ToolDefinition = {
  name: 'move_document',
  description: 'Move or rename a document to a new location.',
  parameters: {
    type: 'object',
    properties: {
      oldPath: {
        type: 'string',
        description: 'The current path of the document.',
      },
      newPath: {
        type: 'string',
        description: 'The new path for the document.',
      },
    },
    required: ['oldPath', 'newPath'],
  },
};

export const deleteDocumentTool: ToolDefinition = {
  name: 'delete_document',
  description:
    'Delete a document. This action requires user confirmation and moves the file to trash.',
  parameters: {
    type: 'object',
    properties: {
      path: {
        type: 'string',
        description: 'The path to the document to delete.',
      },
      reason: {
        type: 'string',
        description: 'The reason for deleting this document.',
      },
    },
    required: ['path'],
  },
};

export const searchDocumentsTool: ToolDefinition = {
  name: 'search_documents',
  description:
    'Search for documents containing specific text or matching a pattern.',
  parameters: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description: 'The search query to find in documents.',
      },
      filePattern: {
        type: 'string',
        description:
          'Optional glob pattern to filter files (e.g., "*.midlight" or "notes/*").',
      },
    },
    required: ['query'],
  },
};

// ============================================================================
// All Tools
// ============================================================================

export const allAgentTools: ToolDefinition[] = [
  listDocumentsTool,
  readDocumentTool,
  createDocumentTool,
  editDocumentTool,
  moveDocumentTool,
  deleteDocumentTool,
  searchDocumentsTool,
];

// Non-destructive tools that don't require confirmation
export const safeAgentTools: ToolDefinition[] = [
  listDocumentsTool,
  readDocumentTool,
  searchDocumentsTool,
];

// Tools that modify documents (require confirmation for delete)
export const modifyingTools: ToolDefinition[] = [
  createDocumentTool,
  editDocumentTool,
  moveDocumentTool,
  deleteDocumentTool,
];

// Tool name type
export type AgentToolName =
  | 'list_documents'
  | 'read_document'
  | 'create_document'
  | 'edit_document'
  | 'move_document'
  | 'delete_document'
  | 'search_documents';

// Check if a tool is destructive
export function isDestructiveTool(name: string): boolean {
  return name === 'delete_document';
}

// Check if a tool modifies documents
export function isModifyingTool(name: string): boolean {
  return ['create_document', 'edit_document', 'move_document', 'delete_document'].includes(name);
}
