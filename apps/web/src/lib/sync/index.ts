// Sync module - Handles offline sync queue and cloud sync operations

export {
  SyncQueue,
  getSyncQueue,
  destroySyncQueue,
  type PendingOperation,
  type OperationType,
} from './queue';

export {
  syncClient,
  type SyncDocument,
  type SyncConflict,
  type SyncUsage,
  type SyncStatus,
  type DocumentContent,
  type ConflictDetails,
  type SyncResult,
  type ConflictResolution,
} from './client';

export {
  syncManager,
  type LocalDocument,
  type SyncManagerOptions,
} from './manager';

export {
  initSyncIntegration,
  destroySyncIntegration,
  queueForSync,
  forceSyncDocument,
  pullDocument,
  deleteFromCloud,
  triggerFullSync,
  hasUnsyncedChanges,
  createSyncedSaveDocument,
} from './integration';
