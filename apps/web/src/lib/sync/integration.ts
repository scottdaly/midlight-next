// Sync Integration - Hooks cloud sync into local storage operations

import { get } from 'svelte/store';
import { sync, auth, network } from '@midlight/stores';
import { syncManager, type LocalDocument } from './manager';
import type { TiptapDocument, SidecarDocument, SaveResult } from '@midlight/core/types';
import { DocumentSerializer } from '@midlight/core/serialization';

// Debounce cloud sync to avoid excessive API calls
const SYNC_DEBOUNCE_MS = 2000;
const syncTimers = new Map<string, ReturnType<typeof setTimeout>>();

/**
 * Initialize sync integration
 * Call this after the storage adapter is ready and user is authenticated
 */
export async function initSyncIntegration(): Promise<void> {
  const authState = get(auth);

  if (!authState.isAuthenticated) {
    console.log('[SyncIntegration] Not authenticated, skipping init');
    return;
  }

  try {
    await syncManager.init({
      autoSyncEnabled: true,
      autoSyncIntervalMs: 30000, // 30 seconds
    });
    console.log('[SyncIntegration] Initialized');
  } catch (error) {
    console.error('[SyncIntegration] Init error:', error);
  }
}

/**
 * Destroy sync integration
 */
export function destroySyncIntegration(): void {
  // Clear all pending sync timers
  for (const timer of syncTimers.values()) {
    clearTimeout(timer);
  }
  syncTimers.clear();

  syncManager.destroy();
  console.log('[SyncIntegration] Destroyed');
}

/**
 * Queue a document for cloud sync after local save
 * Uses debouncing to batch rapid saves
 */
export function queueForSync(
  path: string,
  content: string,
  sidecar: SidecarDocument
): void {
  const syncState = get(sync);

  // Skip if sync is disabled
  if (!syncState.enabled) {
    return;
  }

  // Clear existing timer for this path
  const existingTimer = syncTimers.get(path);
  if (existingTimer) {
    clearTimeout(existingTimer);
  }

  // Set new debounce timer
  const timer = setTimeout(() => {
    syncTimers.delete(path);
    performCloudSync(path, content, sidecar);
  }, SYNC_DEBOUNCE_MS);

  syncTimers.set(path, timer);
}

/**
 * Perform the actual cloud sync for a document
 */
async function performCloudSync(
  path: string,
  content: string,
  sidecar: SidecarDocument
): Promise<void> {
  const authState = get(auth);
  const networkState = get(network);
  const syncState = get(sync);

  // Skip if conditions aren't right
  if (!authState.isAuthenticated || !networkState.online || !syncState.enabled) {
    console.log('[SyncIntegration] Skipping sync - conditions not met');
    return;
  }

  // Get the remote document version for conflict detection
  const remoteDoc = syncState.remoteDocuments.find(
    (d) => d.path === path && !d.deleted
  );
  const baseVersion = remoteDoc?.version;

  try {
    const doc: LocalDocument = {
      path,
      content,
      sidecar: sidecar as unknown as Record<string, unknown>,
    };

    const success = await syncManager.uploadDocument(doc, baseVersion);

    if (success) {
      console.log('[SyncIntegration] Synced:', path);
    } else {
      console.log('[SyncIntegration] Sync queued or conflict:', path);
    }
  } catch (error) {
    console.error('[SyncIntegration] Sync error:', error);
  }
}

/**
 * Force immediate sync for a document (bypasses debounce)
 */
export async function forceSyncDocument(
  path: string,
  content: string,
  sidecar: SidecarDocument
): Promise<boolean> {
  // Clear any pending timer
  const existingTimer = syncTimers.get(path);
  if (existingTimer) {
    clearTimeout(existingTimer);
    syncTimers.delete(path);
  }

  const syncState = get(sync);
  const remoteDoc = syncState.remoteDocuments.find(
    (d) => d.path === path && !d.deleted
  );

  const doc: LocalDocument = {
    path,
    content,
    sidecar: sidecar as unknown as Record<string, unknown>,
  };

  return syncManager.uploadDocument(doc, remoteDoc?.version);
}

/**
 * Pull latest version of a document from cloud
 */
export async function pullDocument(documentId: string): Promise<LocalDocument | null> {
  return syncManager.downloadDocument(documentId);
}

/**
 * Delete a document from cloud
 */
export async function deleteFromCloud(path: string): Promise<boolean> {
  const syncState = get(sync);
  const remoteDoc = syncState.remoteDocuments.find(
    (d) => d.path === path && !d.deleted
  );

  if (!remoteDoc) {
    console.log('[SyncIntegration] Document not in cloud:', path);
    return true;
  }

  return syncManager.deleteDocument(remoteDoc.id);
}

/**
 * Trigger a manual full sync
 */
export async function triggerFullSync(): Promise<void> {
  await syncManager.performSync();
}

/**
 * Check if a document has unsync'd changes
 */
export async function hasUnsyncedChanges(path: string, content: string): Promise<boolean> {
  return syncManager.needsSync(path, content);
}

/**
 * Create a wrapped save function that integrates cloud sync
 */
export function createSyncedSaveDocument(
  originalSave: (
    workspaceRoot: string,
    filePath: string,
    json: TiptapDocument,
    trigger: 'autosave' | 'manual' | 'close' | 'timer' | 'bookmark'
  ) => Promise<SaveResult>
) {
  const serializer = new DocumentSerializer({
    storeImage: async (dataUrl) => {
      // Just return the reference - actual storage is handled by the original save
      return dataUrl;
    },
  });

  return async function syncedSaveDocument(
    workspaceRoot: string,
    filePath: string,
    json: TiptapDocument,
    trigger: 'autosave' | 'manual' | 'close' | 'timer' | 'bookmark'
  ): Promise<SaveResult> {
    // First, do the local save
    const result = await originalSave(workspaceRoot, filePath, json, trigger);

    // If local save succeeded, queue for cloud sync
    if (result.success) {
      try {
        // Serialize for cloud sync
        const { markdown, sidecar } = await serializer.serialize(json);
        queueForSync(filePath, markdown, sidecar);
      } catch (error) {
        console.error('[SyncIntegration] Error preparing sync:', error);
        // Don't fail the save just because sync prep failed
      }
    }

    return result;
  };
}
