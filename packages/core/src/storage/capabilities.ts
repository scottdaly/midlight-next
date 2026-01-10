// Storage capability detection for browser environments

import { formatBytes } from '../utils/index.js';

export interface StorageCapabilities {
  opfs: boolean;
  indexedDb: boolean;
  serviceWorker: boolean;
  storageEstimate: StorageEstimate | null;
}

export interface StorageEstimate {
  quota: number;
  usage: number;
  usageDetails?: {
    indexedDB?: number;
    caches?: number;
    serviceWorkerRegistrations?: number;
  };
}

export interface StorageUsageInfo {
  totalQuota: number;
  totalUsage: number;
  percentUsed: number;
  formattedQuota: string;
  formattedUsage: string;
  isLow: boolean;
  isCritical: boolean;
}

/**
 * Detect available storage capabilities in the browser
 */
export async function detectStorageCapabilities(): Promise<StorageCapabilities> {
  const capabilities: StorageCapabilities = {
    opfs: false,
    indexedDb: false,
    serviceWorker: false,
    storageEstimate: null,
  };

  // Check OPFS support
  if (typeof navigator !== 'undefined' && 'storage' in navigator) {
    const storageManager = navigator.storage;
    if (typeof storageManager.getDirectory === 'function') {
      try {
        await storageManager.getDirectory();
        capabilities.opfs = true;
      } catch {
        // OPFS not available or blocked
      }
    }
  }

  // Check IndexedDB support
  if (typeof indexedDB !== 'undefined') {
    try {
      // Quick test to verify IndexedDB works
      const testDbName = '__midlight_idb_test__';
      const request = indexedDB.open(testDbName, 1);
      await new Promise<void>((resolve, reject) => {
        request.onerror = () => reject(request.error);
        request.onsuccess = () => {
          request.result.close();
          indexedDB.deleteDatabase(testDbName);
          resolve();
        };
      });
      capabilities.indexedDb = true;
    } catch {
      // IndexedDB not available or blocked
    }
  }

  // Check Service Worker support
  if (typeof navigator !== 'undefined' && 'serviceWorker' in navigator) {
    capabilities.serviceWorker = true;
  }

  // Get storage estimate
  if (typeof navigator !== 'undefined' && 'storage' in navigator) {
    const storageManager = navigator.storage;
    if (typeof storageManager.estimate === 'function') {
      try {
        const estimate = await storageManager.estimate();
        capabilities.storageEstimate = {
          quota: estimate.quota ?? 0,
          usage: estimate.usage ?? 0,
          usageDetails: (estimate as StorageEstimate).usageDetails,
        };
      } catch {
        // Storage estimate not available
      }
    }
  }

  return capabilities;
}

/**
 * Get detailed storage usage information
 */
export async function getStorageUsageInfo(): Promise<StorageUsageInfo | null> {
  if (typeof navigator === 'undefined' || !('storage' in navigator)) {
    return null;
  }

  const storageManager = navigator.storage;

  if (typeof storageManager.estimate !== 'function') {
    return null;
  }

  try {
    const estimate = await storageManager.estimate();
    const quota = estimate.quota ?? 0;
    const usage = estimate.usage ?? 0;
    const percentUsed = quota > 0 ? (usage / quota) * 100 : 0;

    return {
      totalQuota: quota,
      totalUsage: usage,
      percentUsed,
      formattedQuota: formatBytes(quota),
      formattedUsage: formatBytes(usage),
      isLow: percentUsed > 80,
      isCritical: percentUsed > 95,
    };
  } catch {
    return null;
  }
}

/**
 * Request persistent storage (prevents browser from clearing storage)
 */
export async function requestPersistentStorage(): Promise<boolean> {
  if (typeof navigator === 'undefined' || !('storage' in navigator)) {
    return false;
  }

  const storageManager = navigator.storage;

  if (typeof storageManager.persist !== 'function') {
    return false;
  }

  try {
    // Check if already persisted
    if (typeof storageManager.persisted === 'function') {
      const alreadyPersisted = await storageManager.persisted();
      if (alreadyPersisted) return true;
    }

    // Request persistence
    return await storageManager.persist();
  } catch {
    return false;
  }
}

/**
 * Check if storage is persisted
 */
export async function isStoragePersisted(): Promise<boolean> {
  if (typeof navigator === 'undefined' || !('storage' in navigator)) {
    return false;
  }

  const storageManager = navigator.storage;

  if (typeof storageManager.persisted !== 'function') {
    return false;
  }

  try {
    return await storageManager.persisted();
  } catch {
    return false;
  }
}

// Re-export formatBytes from utils for convenience
export { formatBytes };
