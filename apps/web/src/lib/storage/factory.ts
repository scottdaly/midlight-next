// Storage Adapter Factory - Creates the appropriate storage adapter based on browser capabilities

import type { StorageAdapter } from '@midlight/core/types';
import { detectStorageCapabilities, type StorageCapabilities } from '@midlight/core/storage';
import { WebStorageAdapter } from './adapter';
import { IndexedDBStorageAdapter } from './indexeddb-adapter';

export type StorageType = 'opfs' | 'indexeddb';

export interface StorageFactoryResult {
  adapter: StorageAdapter;
  type: StorageType;
  capabilities: StorageCapabilities;
}

let cachedResult: StorageFactoryResult | null = null;
let initPromise: Promise<StorageFactoryResult> | null = null;

/**
 * Creates the appropriate storage adapter based on browser capabilities.
 * Uses OPFS if available (best performance), falls back to IndexedDB.
 *
 * The result is cached, so subsequent calls return the same adapter.
 * Handles concurrent calls by returning the same in-flight promise.
 */
export async function createStorageAdapter(): Promise<StorageFactoryResult> {
  // Return cached adapter if available
  if (cachedResult) {
    return cachedResult;
  }

  // Return in-flight promise if initialization is in progress
  if (initPromise) {
    return initPromise;
  }

  // Start initialization and track the promise
  initPromise = initializeStorageAdapter();

  try {
    cachedResult = await initPromise;
    return cachedResult;
  } finally {
    initPromise = null;
  }
}

/**
 * Internal initialization logic - separated to enable promise tracking
 */
async function initializeStorageAdapter(): Promise<StorageFactoryResult> {
  const capabilities = await detectStorageCapabilities();

  let adapter: StorageAdapter;
  let type: StorageType;

  if (capabilities.opfs) {
    // Use OPFS - best performance
    adapter = new WebStorageAdapter();
    type = 'opfs';
  } else if (capabilities.indexedDb) {
    // Fallback to IndexedDB
    adapter = new IndexedDBStorageAdapter();
    type = 'indexeddb';
  } else {
    // No storage available - this is a critical error
    throw new Error(
      'No supported storage mechanism available. ' +
        'Midlight requires either Origin Private File System (OPFS) or IndexedDB.'
    );
  }

  // Initialize the adapter
  await adapter.init();

  return { adapter, type, capabilities };
}

/**
 * Get the current storage adapter without creating a new one.
 * Returns null if no adapter has been created yet.
 */
export function getCurrentStorageAdapter(): StorageFactoryResult | null {
  return cachedResult;
}

/**
 * Clear the cached adapter. Useful for testing or reinitializing storage.
 */
export function clearStorageAdapterCache(): void {
  cachedResult = null;
}

/**
 * Get a human-readable description of the storage type.
 */
export function getStorageTypeDescription(type: StorageType): string {
  switch (type) {
    case 'opfs':
      return 'Origin Private File System (High Performance)';
    case 'indexeddb':
      return 'IndexedDB (Compatibility Mode)';
  }
}

/**
 * Check if the storage type supports all features.
 * OPFS supports all features, IndexedDB may have limitations.
 */
export function getStorageFeatures(type: StorageType): {
  supportsLargeFiles: boolean;
  supportsDirectoryWatching: boolean;
  estimatedPerformance: 'high' | 'medium' | 'low';
} {
  switch (type) {
    case 'opfs':
      return {
        supportsLargeFiles: true,
        supportsDirectoryWatching: false, // Not supported in any browser yet
        estimatedPerformance: 'high',
      };
    case 'indexeddb':
      return {
        supportsLargeFiles: false, // May hit quota limits sooner
        supportsDirectoryWatching: false,
        estimatedPerformance: 'medium',
      };
  }
}
