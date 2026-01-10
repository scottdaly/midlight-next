// @midlight/core/types - Type definitions

// Midlight Document Format (.midlight files)
export interface MidlightDocument {
  version: 1;
  meta: MidlightMeta;
  document: MidlightDocumentSettings;
  content: TiptapDocument;
  images?: Record<string, MidlightImageInfo>;
}

export interface MidlightMeta {
  created: string;
  modified: string;
  title?: string;
  author?: string;
  tags?: string[];
}

export interface MidlightDocumentSettings {
  defaultFont?: string;
  defaultFontSize?: number;
  lineHeight?: number;
  paragraphSpacing?: number;
}

export interface MidlightImageInfo {
  hash: string;
  originalName?: string;
  width?: number;
  height?: number;
}

// Tiptap Document types
export interface TiptapDocument {
  type: 'doc';
  content: TiptapNode[];
}

export interface TiptapNode {
  type: string;
  attrs?: Record<string, unknown>;
  content?: TiptapNode[];
  marks?: TiptapMark[];
  text?: string;
}

export interface TiptapMark {
  type: string;
  attrs?: Record<string, unknown>;
}

// Sidecar types (formatting metadata)
export interface SidecarDocument {
  version: 1;
  meta: SidecarMeta;
  document: DocumentSettings;
  blocks: Record<string, BlockFormatting>;
  spans: Record<string, SpanFormatting[]>;
  images: Record<string, ImageInfo>;
}

export interface SidecarMeta {
  created: string;
  modified: string;
  title?: string;
  author?: string;
  wordCount?: number;
  readingTime?: number;
}

export interface DocumentSettings {
  defaultFont?: string;
  defaultFontSize?: string;
  lineHeight?: number;
  pageSize?: 'A4' | 'Letter';
  pageMargins?: {
    top: number;
    right: number;
    bottom: number;
    left: number;
  };
}

export interface BlockFormatting {
  textAlign?: 'left' | 'center' | 'right' | 'justify';
  indent?: number;
  lineSpacing?: number;
  backgroundColor?: string;
  borderLeft?: string;
  listStyle?: string;
}

export interface SpanFormatting {
  start: number;
  end: number;
  fontFamily?: string;
  fontSize?: string;
  color?: string;
  backgroundColor?: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strikethrough?: boolean;
  superscript?: boolean;
  subscript?: boolean;
  code?: boolean;
  link?: string;
}

export interface ImageInfo {
  ref: string;
  alt?: string;
  title?: string;
  width?: number;
  height?: number;
  alignment?: 'left' | 'center' | 'right';
}

// Checkpoint/Version types
export interface Checkpoint {
  id: string;
  contentHash: string;
  sidecarHash: string;
  timestamp: string;
  parentId: string | null;
  type: 'auto' | 'bookmark';
  label?: string;
  description?: string;
  stats: CheckpointStats;
  trigger: CheckpointTrigger;
}

export interface CheckpointStats {
  wordCount: number;
  charCount: number;
  changeSize: number;
}

export type CheckpointTrigger =
  | 'file_open'
  | 'interval'
  | 'significant_change'
  | 'file_close'
  | 'bookmark'
  | 'before_restore'
  | 'manual';

export interface CheckpointHistory {
  fileKey: string;
  headId: string | null;
  checkpoints: Checkpoint[];
}

// Workspace types
export interface WorkspaceConfig {
  version: 1;
  versioning: VersioningConfig;
  editor: EditorConfig;
  recovery: RecoveryConfig;
}

export interface VersioningConfig {
  enabled: boolean;
  autoCheckpointInterval: number;
  minChangeThreshold: number;
  maxCheckpointsPerFile: number;
  retentionDays: number;
}

export interface EditorConfig {
  defaultFont: string;
  defaultFontSize: string;
  spellcheck: boolean;
  autoSave: boolean;
  autoSaveInterval: number;
}

export interface RecoveryConfig {
  enabled: boolean;
  walInterval: number;
}

// File system types
export interface FileNode {
  id: string;
  name: string;
  path: string;
  type: 'file' | 'directory';
  category?: FileCategory;
  children?: FileNode[];
}

export type FileCategory =
  | 'midlight'    // .midlight files - native format
  | 'native'      // .md files we can fully edit (legacy)
  | 'compatible'  // .txt, .json - basic editing
  | 'importable'  // .docx - can import
  | 'viewable'    // images, pdf - can display
  | 'unsupported'; // everything else

// AI/Chat types
export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string;
  toolActions?: ToolAction[];
  thinkingSteps?: ThinkingStep[];
  documentChanges?: DocumentChange[];
}

export interface Conversation {
  id: string;
  title: string;
  messages: Message[];
  createdAt: string;
  updatedAt: string;
}

export interface ToolAction {
  id: string;
  type: 'create' | 'edit' | 'delete' | 'move' | 'read' | 'list' | 'search' | 'web_search';
  label: string;
  path?: string;
  status: 'pending' | 'running' | 'complete' | 'error';
  result?: unknown;
}

export type ThinkingStepStatus = 'pending' | 'active' | 'completed';

export type ThinkingStepIcon =
  | 'analyze'
  | 'read'
  | 'search'
  | 'web_search'
  | 'create'
  | 'edit'
  | 'folder'
  | 'delete'
  | 'move'
  | 'thinking';

export interface ThinkingStep {
  id: string;
  label: string;
  icon: ThinkingStepIcon;
  status: ThinkingStepStatus;
  timestamp: number;
}

export interface DocumentChange {
  type: 'create' | 'edit' | 'move' | 'delete';
  path: string;
  newPath?: string;
  contentBefore?: string;
  contentAfter?: string;
  changeId?: string;
}

// Storage adapter interface (platform-agnostic)
export interface StorageAdapter {
  // Lifecycle
  init(): Promise<void>;

  // File operations
  readDir(path: string): Promise<FileNode[]>;
  readFile(path: string): Promise<string>;
  writeFile(path: string, content: string): Promise<void>;
  deleteFile(path: string): Promise<void>;
  renameFile(oldPath: string, newPath: string): Promise<void>;
  fileExists(path: string): Promise<boolean>;
  createFile(parentPath: string, name: string): Promise<FileNode>;
  createFolder(parentPath: string, name: string): Promise<FileNode>;

  // Document operations
  loadDocument(workspaceRoot: string, filePath: string): Promise<LoadedDocument>;
  saveDocument(
    workspaceRoot: string,
    filePath: string,
    json: TiptapDocument,
    trigger: CheckpointTrigger
  ): Promise<SaveResult>;

  // Workspace operations
  initWorkspace(path: string): Promise<void>;
  getCheckpoints(workspaceRoot: string, filePath: string): Promise<Checkpoint[]>;
  restoreCheckpoint(workspaceRoot: string, filePath: string, checkpointId: string): Promise<TiptapDocument>;
}

export interface LoadedDocument {
  json: TiptapDocument;
  sidecar: SidecarDocument;
  hasRecovery: boolean;
  recoveryTime?: string;
}

export interface SaveResult {
  success: boolean;
  checkpointId?: string;
  error?: string;
}
