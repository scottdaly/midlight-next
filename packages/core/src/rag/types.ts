// RAG (Retrieval Augmented Generation) Types
// Used for cross-project semantic search and context retrieval

/**
 * A chunk of content extracted from a document for embedding
 */
export interface DocumentChunk {
  /** Unique ID for the chunk (hash of content + position) */
  id: string;
  /** Absolute path to the project root */
  projectPath: string;
  /** Absolute path to the source file */
  filePath: string;
  /** Index of this chunk within the file (0-based) */
  chunkIndex: number;
  /** The actual text content of the chunk */
  content: string;
  /** Additional metadata about the chunk */
  metadata: ChunkMetadata;
  /** When this chunk was created/last updated */
  createdAt: string;
}

/**
 * Metadata associated with a chunk
 */
export interface ChunkMetadata {
  /** The heading this chunk falls under (if any) */
  heading?: string;
  /** The section path (e.g., "Introduction > Background") */
  section?: string;
  /** Estimated token count for this chunk */
  tokenEstimate?: number;
  /** Character offset in the original document */
  charOffset?: number;
}

/**
 * A chunk with its embedding vector
 */
export interface EmbeddedChunk extends DocumentChunk {
  /** The embedding vector (1536 dimensions for text-embedding-3-small) */
  embedding: number[];
}

/**
 * A search result from the vector store
 */
export interface SearchResult {
  /** The matching chunk */
  chunk: DocumentChunk;
  /** Cosine similarity score (0-1, higher is more similar) */
  score: number;
}

/**
 * Status of the index for a project
 */
export interface IndexStatus {
  /** Path to the project */
  projectPath: string;
  /** Project name from config */
  projectName?: string;
  /** Total number of documents in the project */
  totalDocuments: number;
  /** Number of documents that have been indexed */
  indexedDocuments: number;
  /** Total number of chunks in the index */
  totalChunks: number;
  /** When the index was last updated */
  lastIndexed?: string;
  /** Whether indexing is currently in progress */
  isIndexing: boolean;
  /** Any error that occurred during indexing */
  error?: string;
}

/**
 * Options for chunking a document
 */
export interface ChunkOptions {
  /** Maximum tokens per chunk (default: 500) */
  maxChunkTokens: number;
  /** Number of tokens to overlap between chunks (default: 50) */
  overlapTokens: number;
  /** Whether to preserve heading context in each chunk */
  preserveHeadings: boolean;
  /** Minimum chunk size in tokens (to avoid tiny chunks) */
  minChunkTokens: number;
}

/**
 * Default chunking options
 */
export const DEFAULT_CHUNK_OPTIONS: ChunkOptions = {
  maxChunkTokens: 500,
  overlapTokens: 50,
  preserveHeadings: true,
  minChunkTokens: 50,
};

/**
 * Options for semantic search
 */
export interface SearchOptions {
  /** Maximum number of results to return */
  topK: number;
  /** Minimum similarity score threshold (0-1) */
  minScore: number;
  /** Filter to specific projects (if empty, search all) */
  projectPaths?: string[];
  /** Filter to specific file extensions */
  fileExtensions?: string[];
}

/**
 * Default search options
 */
export const DEFAULT_SEARCH_OPTIONS: SearchOptions = {
  topK: 5,
  minScore: 0.7,
};

/**
 * Request to index a project
 */
export interface IndexProjectRequest {
  projectPath: string;
  /** Force re-indexing even if already indexed */
  force?: boolean;
}

/**
 * Request for semantic search
 */
export interface SearchRequest {
  query: string;
  options?: Partial<SearchOptions>;
}
