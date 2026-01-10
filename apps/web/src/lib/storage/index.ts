// Storage module - Web storage adapters with automatic capability detection

export { WebStorageAdapter } from './adapter';
export { IndexedDBStorageAdapter } from './indexeddb-adapter';
export {
  createStorageAdapter,
  getCurrentStorageAdapter,
  clearStorageAdapterCache,
  getStorageTypeDescription,
  getStorageFeatures,
  type StorageType,
  type StorageFactoryResult,
} from './factory';
