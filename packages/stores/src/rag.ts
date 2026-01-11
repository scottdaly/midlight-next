// @midlight/stores/rag - RAG (Retrieval Augmented Generation) state management

import { writable, derived, get } from 'svelte/store';
import type {
  IndexStatus,
  SearchResult,
  SearchOptions,
  DEFAULT_SEARCH_OPTIONS,
} from '@midlight/core';

export interface RAGState {
  /** Index status for each project by path */
  indexStatus: Map<string, IndexStatus>;
  /** Whether a search is currently in progress */
  isSearching: boolean;
  /** Results from the last search */
  searchResults: SearchResult[];
  /** The last search query */
  searchQuery: string;
  /** Any error from the last operation */
  error: string | null;
}

const initialState: RAGState = {
  indexStatus: new Map(),
  isSearching: false,
  searchResults: [],
  searchQuery: '',
  error: null,
};

// RAG operations (injected from platform layer)
export type RAGIndexer = (projectPath: string, force?: boolean) => Promise<IndexStatus>;
export type RAGSearcher = (query: string, options?: Partial<SearchOptions>) => Promise<SearchResult[]>;
export type RAGStatusGetter = (projectPath?: string) => Promise<IndexStatus[]>;
export type RAGIndexDeleter = (projectPath: string) => Promise<void>;

function createRAGStore() {
  const { subscribe, set, update } = writable<RAGState>(initialState);

  // Platform-specific implementations (injected at runtime)
  let indexer: RAGIndexer | null = null;
  let searcher: RAGSearcher | null = null;
  let statusGetter: RAGStatusGetter | null = null;
  let indexDeleter: RAGIndexDeleter | null = null;

  return {
    subscribe,

    /**
     * Sets the RAG implementation functions (Tauri or Web)
     */
    setImplementation(impl: {
      indexer: RAGIndexer;
      searcher: RAGSearcher;
      statusGetter: RAGStatusGetter;
      indexDeleter: RAGIndexDeleter;
    }) {
      indexer = impl.indexer;
      searcher = impl.searcher;
      statusGetter = impl.statusGetter;
      indexDeleter = impl.indexDeleter;
    },

    /**
     * Index a project for semantic search
     */
    async indexProject(projectPath: string, force = false): Promise<IndexStatus | null> {
      if (!indexer) {
        update((s) => ({ ...s, error: 'RAG indexer not initialized' }));
        return null;
      }

      // Set indexing status
      update((s) => {
        const newStatus = new Map(s.indexStatus);
        const current = newStatus.get(projectPath);
        newStatus.set(projectPath, {
          ...(current || {
            projectPath,
            totalDocuments: 0,
            indexedDocuments: 0,
            totalChunks: 0,
          }),
          isIndexing: true,
          error: undefined,
        });
        return { ...s, indexStatus: newStatus, error: null };
      });

      try {
        const status = await indexer(projectPath, force);
        update((s) => {
          const newStatus = new Map(s.indexStatus);
          newStatus.set(projectPath, status);
          return { ...s, indexStatus: newStatus };
        });
        return status;
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        update((s) => {
          const newStatus = new Map(s.indexStatus);
          const current = newStatus.get(projectPath);
          if (current) {
            newStatus.set(projectPath, { ...current, isIndexing: false, error: errorMsg });
          }
          return { ...s, indexStatus: newStatus, error: errorMsg };
        });
        return null;
      }
    },

    /**
     * Search across indexed projects
     */
    async search(query: string, options?: Partial<SearchOptions>): Promise<SearchResult[]> {
      if (!searcher) {
        update((s) => ({ ...s, error: 'RAG searcher not initialized' }));
        return [];
      }

      update((s) => ({ ...s, isSearching: true, searchQuery: query, error: null }));

      try {
        const results = await searcher(query, options);
        update((s) => ({ ...s, isSearching: false, searchResults: results }));
        return results;
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        update((s) => ({ ...s, isSearching: false, error: errorMsg, searchResults: [] }));
        return [];
      }
    },

    /**
     * Get index status for all or specific projects
     */
    async refreshStatus(projectPath?: string): Promise<void> {
      if (!statusGetter) return;

      try {
        const statuses = await statusGetter(projectPath);
        update((s) => {
          const newStatus = new Map(s.indexStatus);
          for (const status of statuses) {
            newStatus.set(status.projectPath, status);
          }
          return { ...s, indexStatus: newStatus };
        });
      } catch (error) {
        console.error('Failed to refresh RAG status:', error);
      }
    },

    /**
     * Delete index for a project
     */
    async deleteIndex(projectPath: string): Promise<void> {
      if (!indexDeleter) return;

      try {
        await indexDeleter(projectPath);
        update((s) => {
          const newStatus = new Map(s.indexStatus);
          newStatus.delete(projectPath);
          return { ...s, indexStatus: newStatus };
        });
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        update((s) => ({ ...s, error: errorMsg }));
      }
    },

    /**
     * Clear search results
     */
    clearSearch() {
      update((s) => ({ ...s, searchResults: [], searchQuery: '' }));
    },

    /**
     * Reset to initial state
     */
    reset() {
      set(initialState);
    },
  };
}

export const rag = createRAGStore();

// Derived stores for convenient access
export const isSearching = derived(rag, ($rag) => $rag.isSearching);
export const searchResults = derived(rag, ($rag) => $rag.searchResults);
export const searchQuery = derived(rag, ($rag) => $rag.searchQuery);
export const ragError = derived(rag, ($rag) => $rag.error);

export const indexedProjectCount = derived(
  rag,
  ($rag) => [...$rag.indexStatus.values()].filter((s) => s.totalChunks > 0).length
);

export const isAnyProjectIndexing = derived(rag, ($rag) =>
  [...$rag.indexStatus.values()].some((s) => s.isIndexing)
);
