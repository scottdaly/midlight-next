// Sync Queue - Stores pending operations for offline support
// Operations are persisted in IndexedDB and processed when back online

import { openDB, type IDBPDatabase } from 'idb';
import { network } from '@midlight/stores';

export type OperationType = 'create' | 'update' | 'delete' | 'rename' | 'move';

export interface PendingOperation {
  id: string;
  type: OperationType;
  path: string;
  payload: unknown;
  timestamp: number;
  retryCount: number;
  lastError?: string;
}

interface SyncQueueDB {
  operations: {
    key: string;
    value: PendingOperation;
    indexes: {
      'by-timestamp': number;
      'by-path': string;
    };
  };
}

export class SyncQueue {
  private db: IDBPDatabase<SyncQueueDB> | null = null;
  private isProcessing = false;
  private processInterval: ReturnType<typeof setInterval> | null = null;
  private onlineHandler: (() => void) | null = null;

  /**
   * Initialize the sync queue database
   */
  async init(): Promise<void> {
    this.db = await openDB<SyncQueueDB>('midlight-sync-queue', 1, {
      upgrade(db) {
        if (!db.objectStoreNames.contains('operations')) {
          const store = db.createObjectStore('operations', { keyPath: 'id' });
          store.createIndex('by-timestamp', 'timestamp');
          store.createIndex('by-path', 'path');
        }
      },
    });

    // Start processing queue periodically when online
    this.startProcessing();
  }

  /**
   * Add an operation to the queue
   */
  async enqueue(
    type: OperationType,
    path: string,
    payload: unknown
  ): Promise<string> {
    if (!this.db) throw new Error('SyncQueue not initialized');

    const id = crypto.randomUUID();
    const operation: PendingOperation = {
      id,
      type,
      path,
      payload,
      timestamp: Date.now(),
      retryCount: 0,
    };

    await this.db.put('operations', operation);
    network.incrementPendingSync();

    // Try to process immediately if online
    this.processQueue();

    return id;
  }

  /**
   * Get all pending operations
   */
  async getPending(): Promise<PendingOperation[]> {
    if (!this.db) throw new Error('SyncQueue not initialized');

    return this.db.getAllFromIndex('operations', 'by-timestamp');
  }

  /**
   * Get pending operations for a specific path
   */
  async getPendingForPath(path: string): Promise<PendingOperation[]> {
    if (!this.db) throw new Error('SyncQueue not initialized');

    return this.db.getAllFromIndex('operations', 'by-path', path);
  }

  /**
   * Remove an operation from the queue (after successful sync)
   */
  async remove(id: string): Promise<void> {
    if (!this.db) throw new Error('SyncQueue not initialized');

    await this.db.delete('operations', id);
    network.decrementPendingSync();
  }

  /**
   * Update retry count and error for an operation
   */
  async markRetry(id: string, error: string): Promise<void> {
    if (!this.db) throw new Error('SyncQueue not initialized');

    const operation = await this.db.get('operations', id);
    if (operation) {
      operation.retryCount++;
      operation.lastError = error;
      await this.db.put('operations', operation);
    }
  }

  /**
   * Clear all pending operations
   */
  async clear(): Promise<void> {
    if (!this.db) throw new Error('SyncQueue not initialized');

    await this.db.clear('operations');
    network.setPendingSyncCount(0);
  }

  /**
   * Get the count of pending operations
   */
  async getCount(): Promise<number> {
    if (!this.db) throw new Error('SyncQueue not initialized');

    return this.db.count('operations');
  }

  /**
   * Start automatic queue processing
   */
  startProcessing(intervalMs = 30000): void {
    this.stopProcessing();

    // Process immediately
    this.processQueue();

    // Then periodically
    this.processInterval = setInterval(() => {
      this.processQueue();
    }, intervalMs);

    // Also process when coming back online
    if (typeof window !== 'undefined') {
      // Store handler reference for cleanup
      this.onlineHandler = () => this.processQueue();
      window.addEventListener('online', this.onlineHandler);
    }
  }

  /**
   * Stop automatic queue processing
   */
  stopProcessing(): void {
    if (this.processInterval) {
      clearInterval(this.processInterval);
      this.processInterval = null;
    }

    // Remove online event listener to prevent memory leak
    if (this.onlineHandler && typeof window !== 'undefined') {
      window.removeEventListener('online', this.onlineHandler);
      this.onlineHandler = null;
    }
  }

  /**
   * Process all pending operations
   * Override this method to implement actual sync logic
   */
  async processQueue(): Promise<void> {
    if (this.isProcessing) return;
    if (typeof navigator !== 'undefined' && !navigator.onLine) return;

    this.isProcessing = true;
    network.setSyncing(true);

    try {
      const operations = await this.getPending();

      for (const operation of operations) {
        // Skip operations that have failed too many times
        if (operation.retryCount >= 5) {
          console.warn(`Operation ${operation.id} exceeded retry limit`);
          continue;
        }

        try {
          await this.processOperation(operation);
          await this.remove(operation.id);
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Unknown error';
          await this.markRetry(operation.id, message);
          network.setSyncError(message);
        }
      }
    } finally {
      this.isProcessing = false;
      network.setSyncing(false);

      // Update pending count
      const count = await this.getCount();
      network.setPendingSyncCount(count);
    }
  }

  /**
   * Process a single operation
   * This is a placeholder - actual sync logic would go here
   */
  protected async processOperation(operation: PendingOperation): Promise<void> {
    // TODO: Implement actual sync to server
    // For now, just log the operation
    console.log('[SyncQueue] Processing operation:', operation);

    // Simulate network request
    // In real implementation, this would call the sync API
    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  /**
   * Destroy the queue and clean up
   */
  destroy(): void {
    this.stopProcessing();
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }
}

// Singleton instance
let queueInstance: SyncQueue | null = null;

/**
 * Get or create the sync queue instance
 */
export async function getSyncQueue(): Promise<SyncQueue> {
  if (!queueInstance) {
    queueInstance = new SyncQueue();
    await queueInstance.init();
  }
  return queueInstance;
}

/**
 * Destroy the sync queue instance
 */
export function destroySyncQueue(): void {
  if (queueInstance) {
    queueInstance.destroy();
    queueInstance = null;
  }
}
