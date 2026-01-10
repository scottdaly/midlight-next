// Sync Manager - Coordinates cloud sync operations

import { get } from 'svelte/store';
import { sync, auth, network, type SyncDocument } from '@midlight/stores';
import { syncClient, type ConflictResolution } from './client';
import { getSyncQueue } from './queue';

export interface LocalDocument {
  path: string;
  content: string;
  sidecar: Record<string, unknown>;
}

export interface SyncManagerOptions {
  autoSyncEnabled?: boolean;
  autoSyncIntervalMs?: number;
  onConflict?: (conflict: SyncDocument) => void;
}

class SyncManager {
  private initialized = false;
  private syncInProgress = false;

  /**
   * Initialize the sync manager
   */
  async init(options: SyncManagerOptions = {}) {
    if (this.initialized) return;

    const authState = get(auth);
    if (!authState.isAuthenticated) {
      console.log('[SyncManager] Not authenticated, skipping init');
      return;
    }

    // Enable sync in store
    sync.enable();

    // Set options
    if (options.autoSyncEnabled !== undefined) {
      sync.setAutoSyncEnabled(options.autoSyncEnabled);
    }
    if (options.autoSyncIntervalMs !== undefined) {
      sync.setAutoSyncInterval(options.autoSyncIntervalMs);
    }

    // Register sync callback for auto-sync
    sync.registerSyncCallback(() => this.performSync());

    // Start auto-sync
    sync.startAutoSync();

    // Initial sync
    await this.performSync();

    // Connect queue to sync
    await this.connectQueue();

    this.initialized = true;
    console.log('[SyncManager] Initialized');
  }

  /**
   * Connect the offline queue to actually perform syncs
   */
  private async connectQueue() {
    const queue = await getSyncQueue();

    // Override the processOperation method to use our sync client
    const originalProcess = queue['processOperation'].bind(queue);
    queue['processOperation'] = async (operation) => {
      const { type, path, payload } = operation;

      switch (type) {
        case 'create':
        case 'update': {
          const doc = payload as LocalDocument;
          const result = await syncClient.uploadDocument(
            doc.path,
            doc.content,
            doc.sidecar
          );

          if (!result.success && result.conflict) {
            // Handle conflict
            sync.addConflict({
              id: result.conflict.id,
              documentId: result.conflict.documentId,
              path: doc.path,
              localVersion: result.conflict.localVersion,
              remoteVersion: result.conflict.remoteVersion,
              createdAt: new Date().toISOString(),
            });

            // Show conflict to user
            sync.setActiveConflict(sync.getDocumentByPath(doc.path) ? {
              id: result.conflict.id,
              documentId: result.conflict.documentId,
              path: doc.path,
              localVersion: result.conflict.localVersion,
              remoteVersion: result.conflict.remoteVersion,
              createdAt: new Date().toISOString(),
            } : null);
          } else if (result.success && result.document) {
            sync.updateRemoteDocument(result.document);
          }
          break;
        }

        case 'delete': {
          const { documentId } = payload as { documentId: string };
          await syncClient.deleteDocument(documentId);
          sync.removeRemoteDocument(documentId);
          break;
        }

        default:
          console.warn('[SyncManager] Unknown operation type:', type);
      }
    };
  }

  /**
   * Perform a full sync - fetch status and reconcile
   */
  async performSync(): Promise<void> {
    if (this.syncInProgress) {
      console.log('[SyncManager] Sync already in progress');
      return;
    }

    const authState = get(auth);
    const networkState = get(network);

    if (!authState.isAuthenticated) {
      console.log('[SyncManager] Not authenticated, skipping sync');
      return;
    }

    if (!networkState.online) {
      console.log('[SyncManager] Offline, skipping sync');
      return;
    }

    this.syncInProgress = true;
    sync.setSyncing(true);

    try {
      // Get remote status
      const status = await syncClient.getStatus();

      // Update store with remote documents
      sync.setRemoteDocuments(status.documents);
      sync.setConflicts(status.conflicts);
      sync.setUsage(status.usage);

      // Record successful sync
      sync.recordSync();

      console.log('[SyncManager] Sync completed:', {
        documents: status.documents.length,
        conflicts: status.conflicts.length,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Sync failed';
      sync.recordSyncError(message);
      console.error('[SyncManager] Sync error:', error);
    } finally {
      this.syncInProgress = false;
      sync.setSyncing(false);
    }
  }

  /**
   * Upload a document to the cloud
   */
  async uploadDocument(doc: LocalDocument, baseVersion?: number): Promise<boolean> {
    const authState = get(auth);
    const networkState = get(network);

    if (!authState.isAuthenticated) {
      console.log('[SyncManager] Not authenticated, queuing for later');
      const queue = await getSyncQueue();
      await queue.enqueue('update', doc.path, doc);
      return false;
    }

    if (!networkState.online) {
      console.log('[SyncManager] Offline, queuing for later');
      const queue = await getSyncQueue();
      await queue.enqueue('update', doc.path, doc);
      return false;
    }

    // Mark as pending upload
    sync.markPendingUpload(doc.path, true);

    try {
      const result = await syncClient.uploadDocument(
        doc.path,
        doc.content,
        doc.sidecar,
        baseVersion
      );

      if (result.success && result.document) {
        sync.updateRemoteDocument(result.document);
        sync.markPendingUpload(doc.path, false);

        // Update local version tracking
        sync.setLocalVersion(doc.path, {
          path: doc.path,
          contentHash: result.document.contentHash,
          sidecarHash: result.document.sidecarHash,
          version: result.document.version,
          lastSyncedAt: new Date().toISOString(),
          pendingUpload: false,
        });

        return true;
      } else if (result.conflict) {
        // Handle conflict
        sync.addConflict({
          id: result.conflict.id,
          documentId: result.conflict.documentId,
          path: doc.path,
          localVersion: result.conflict.localVersion,
          remoteVersion: result.conflict.remoteVersion,
          createdAt: new Date().toISOString(),
        });

        // Show conflict dialog
        sync.setActiveConflict({
          id: result.conflict.id,
          documentId: result.conflict.documentId,
          path: doc.path,
          localVersion: result.conflict.localVersion,
          remoteVersion: result.conflict.remoteVersion,
          createdAt: new Date().toISOString(),
        });

        return false;
      } else {
        sync.recordSyncError(result.error || 'Upload failed');
        return false;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Upload failed';
      sync.recordSyncError(message);
      sync.markPendingUpload(doc.path, false);

      // Queue for retry
      const queue = await getSyncQueue();
      await queue.enqueue('update', doc.path, doc);

      return false;
    }
  }

  /**
   * Download a document from the cloud
   */
  async downloadDocument(documentId: string): Promise<LocalDocument | null> {
    try {
      const doc = await syncClient.downloadDocument(documentId);
      return {
        path: doc.path,
        content: doc.content,
        sidecar: doc.sidecar,
      };
    } catch (error) {
      console.error('[SyncManager] Download error:', error);
      return null;
    }
  }

  /**
   * Delete a document from the cloud
   */
  async deleteDocument(documentId: string): Promise<boolean> {
    const networkState = get(network);

    if (!networkState.online) {
      const queue = await getSyncQueue();
      await queue.enqueue('delete', '', { documentId });
      return false;
    }

    try {
      await syncClient.deleteDocument(documentId);
      sync.removeRemoteDocument(documentId);
      return true;
    } catch (error) {
      console.error('[SyncManager] Delete error:', error);
      const queue = await getSyncQueue();
      await queue.enqueue('delete', '', { documentId });
      return false;
    }
  }

  /**
   * Resolve a conflict
   */
  async resolveConflict(conflictId: string, resolution: ConflictResolution): Promise<boolean> {
    try {
      await syncClient.resolveConflict(conflictId, resolution);
      sync.removeConflict(conflictId);

      // Refresh sync status
      await this.performSync();

      return true;
    } catch (error) {
      console.error('[SyncManager] Conflict resolution error:', error);
      return false;
    }
  }

  /**
   * Get usage stats
   */
  async getUsage() {
    try {
      const usage = await syncClient.getUsage();
      sync.setUsage(usage);
      return usage;
    } catch (error) {
      console.error('[SyncManager] Usage fetch error:', error);
      return null;
    }
  }

  /**
   * Check if a document needs syncing
   */
  async needsSync(path: string, content: string): Promise<boolean> {
    const syncState = get(sync);
    const remoteDoc = syncState.remoteDocuments.find((d) => d.path === path && !d.deleted);

    if (!remoteDoc) {
      // New document, needs sync
      return true;
    }

    // Compare content hashes
    return syncClient.hasLocalChanges(content, remoteDoc.contentHash);
  }

  /**
   * Destroy the sync manager
   */
  destroy() {
    sync.stopAutoSync();
    sync.reset();
    this.initialized = false;
    console.log('[SyncManager] Destroyed');
  }
}

// Singleton instance
export const syncManager = new SyncManager();
