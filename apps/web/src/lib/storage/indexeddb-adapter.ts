// IndexedDB-only Storage Adapter - Fallback for browsers without OPFS support
// Stores all files as blobs in IndexedDB

import { openDB, type IDBPDatabase } from 'idb';
import type {
  StorageAdapter,
  FileNode,
  TiptapDocument,
  SidecarDocument,
  Checkpoint,
  CheckpointTrigger,
  LoadedDocument,
  SaveResult,
  FileCategory,
} from '@midlight/core/types';
import {
  DocumentSerializer,
  DocumentDeserializer,
  createEmptySidecar,
} from '@midlight/core/serialization';
import {
  generateId,
  generateCheckpointId,
  sha256,
  sortFileNodes,
  getExtension,
} from '@midlight/core/utils';

interface IndexedDBOnlySchema {
  files: {
    key: string; // path
    value: {
      path: string;
      content: string;
      isDirectory: boolean;
      createdAt: string;
      updatedAt: string;
    };
    indexes: {
      'by-parent': string;
    };
  };
  images: {
    key: string; // hash
    value: {
      hash: string;
      data: ArrayBuffer;
      mimeType: string;
      createdAt: string;
    };
  };
  documents: {
    key: string;
    value: {
      id: string;
      path: string;
      contentHash: string;
      sidecarHash: string;
      createdAt: string;
      updatedAt: string;
    };
    indexes: {
      'by-path': string;
    };
  };
  objects: {
    key: string;
    value: {
      hash: string;
      content: string;
      createdAt: string;
    };
  };
  checkpoints: {
    key: string;
    value: Checkpoint & { documentId: string };
    indexes: {
      'by-document': string;
      'by-timestamp': string;
    };
  };
  recovery: {
    key: string;
    value: {
      documentId: string;
      content: string;
      timestamp: string;
    };
  };
}

/**
 * IndexedDB-only storage adapter for browsers without OPFS support.
 * Stores files directly in IndexedDB as strings/blobs.
 */
export class IndexedDBStorageAdapter implements StorageAdapter {
  private db: IDBPDatabase<IndexedDBOnlySchema> | null = null;
  private serializer: DocumentSerializer;
  private deserializer: DocumentDeserializer;

  // Write coalescing for rapid saves
  private pendingWrites = new Map<
    string,
    { content: string; resolve: () => void; reject: (error: Error) => void }[]
  >();
  private writeTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private readonly WRITE_DEBOUNCE = 100;

  constructor() {
    this.serializer = new DocumentSerializer({
      storeImage: async (dataUrl) => {
        const hash = await sha256(dataUrl);
        const shortHash = hash.slice(0, 16);
        await this.storeImage(shortHash, dataUrl);
        return `@img:${shortHash}`;
      },
    });

    this.deserializer = new DocumentDeserializer({
      loadImage: async (ref) => {
        if (ref.startsWith('@img:')) {
          const hash = ref.slice(5);
          return this.loadImage(hash);
        }
        return ref;
      },
    });
  }

  async init(): Promise<void> {
    this.db = await openDB<IndexedDBOnlySchema>('midlight-idb', 1, {
      upgrade(db) {
        // Files store - stores all file content
        if (!db.objectStoreNames.contains('files')) {
          const filesStore = db.createObjectStore('files', { keyPath: 'path' });
          filesStore.createIndex('by-parent', 'parent');
        }

        // Images store - stores image blobs
        if (!db.objectStoreNames.contains('images')) {
          db.createObjectStore('images', { keyPath: 'hash' });
        }

        // Documents store - metadata
        if (!db.objectStoreNames.contains('documents')) {
          const docStore = db.createObjectStore('documents', { keyPath: 'id' });
          docStore.createIndex('by-path', 'path', { unique: true });
        }

        // Objects store - content-addressable
        if (!db.objectStoreNames.contains('objects')) {
          db.createObjectStore('objects', { keyPath: 'hash' });
        }

        // Checkpoints store
        if (!db.objectStoreNames.contains('checkpoints')) {
          const cpStore = db.createObjectStore('checkpoints', { keyPath: 'id' });
          cpStore.createIndex('by-document', 'documentId');
          cpStore.createIndex('by-timestamp', 'timestamp');
        }

        // Recovery store
        if (!db.objectStoreNames.contains('recovery')) {
          db.createObjectStore('recovery', { keyPath: 'documentId' });
        }
      },
    });

    // Ensure root directory exists
    await this.ensureDirectory('/');
  }

  private async ensureDirectory(path: string): Promise<void> {
    if (!this.db) throw new Error('Database not initialized');

    const existing = await this.db.get('files', path);
    if (!existing) {
      await this.db.put('files', {
        path,
        content: '',
        isDirectory: true,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      });
    }
  }

  private getParentPath(path: string): string {
    const parts = path.split('/').filter(Boolean);
    parts.pop();
    return parts.length === 0 ? '/' : '/' + parts.join('/');
  }

  private categorizeFile(name: string): FileCategory {
    const ext = getExtension(name);
    switch (ext) {
      case 'md':
        return 'native';
      case 'txt':
      case 'json':
        return 'compatible';
      case 'docx':
        return 'importable';
      case 'png':
      case 'jpg':
      case 'jpeg':
      case 'gif':
      case 'webp':
      case 'svg':
      case 'pdf':
        return 'viewable';
      default:
        return 'unsupported';
    }
  }

  // File operations

  async readDir(path: string): Promise<FileNode[]> {
    if (!this.db) throw new Error('Database not initialized');

    const allFiles = await this.db.getAll('files');
    const entries: FileNode[] = [];

    // Filter files that are direct children of this path
    const normalizedPath = path === '/' ? '' : path;

    for (const file of allFiles) {
      // Skip the path itself
      if (file.path === path) continue;

      // Skip sidecar files
      if (file.path.endsWith('.sidecar.json')) continue;

      // Check if this is a direct child
      const relativePath = file.path.slice(normalizedPath.length);
      if (!relativePath.startsWith('/')) continue;

      const remainingParts = relativePath.slice(1).split('/');
      if (remainingParts.length !== 1) continue;

      const name = remainingParts[0];
      entries.push({
        id: generateId(),
        name,
        path: file.path,
        type: file.isDirectory ? 'directory' : 'file',
        category: file.isDirectory ? undefined : this.categorizeFile(name),
      });
    }

    return sortFileNodes(entries);
  }

  async readFile(path: string): Promise<string> {
    if (!this.db) throw new Error('Database not initialized');

    const file = await this.db.get('files', path);
    if (!file || file.isDirectory) {
      throw new Error(`File not found: ${path}`);
    }

    return file.content;
  }

  async writeFile(path: string, content: string): Promise<void> {
    return new Promise((resolve, reject) => {
      if (!this.pendingWrites.has(path)) {
        this.pendingWrites.set(path, []);
      }
      this.pendingWrites.get(path)!.push({ content, resolve, reject });

      const existingTimer = this.writeTimers.get(path);
      if (existingTimer) {
        clearTimeout(existingTimer);
      }

      const timer = setTimeout(() => {
        this.flushWrite(path);
      }, this.WRITE_DEBOUNCE);

      this.writeTimers.set(path, timer);
    });
  }

  private async flushWrite(path: string): Promise<void> {
    const pending = this.pendingWrites.get(path);
    if (!pending || pending.length === 0) return;

    const lastWrite = pending[pending.length - 1];
    const allPending = [...pending];

    this.pendingWrites.delete(path);
    this.writeTimers.delete(path);

    try {
      await this.writeFileImmediate(path, lastWrite.content);

      for (const write of allPending) {
        write.resolve();
      }
    } catch (error) {
      const err = error instanceof Error ? error : new Error('Write failed');
      for (const write of allPending) {
        write.reject(err);
      }
    }
  }

  private async writeFileImmediate(path: string, content: string): Promise<void> {
    if (!this.db) throw new Error('Database not initialized');

    // Ensure parent directories exist
    const parentPath = this.getParentPath(path);
    if (parentPath !== '/') {
      await this.ensureDirectoryPath(parentPath);
    }

    const now = new Date().toISOString();
    const existing = await this.db.get('files', path);

    await this.db.put('files', {
      path,
      content,
      isDirectory: false,
      createdAt: existing?.createdAt || now,
      updatedAt: now,
    });
  }

  private async ensureDirectoryPath(path: string): Promise<void> {
    const parts = path.split('/').filter(Boolean);
    let currentPath = '';

    for (const part of parts) {
      currentPath = `${currentPath}/${part}`;
      await this.ensureDirectory(currentPath);
    }
  }

  async flushAllWrites(): Promise<void> {
    const paths = Array.from(this.pendingWrites.keys());
    await Promise.all(paths.map((path) => this.flushWrite(path)));
  }

  async deleteFile(path: string): Promise<void> {
    if (!this.db) throw new Error('Database not initialized');

    await this.flushWrite(path);

    await this.db.delete('files', path);

    // Also delete sidecar if exists
    try {
      await this.db.delete('files', `${path}.sidecar.json`);
    } catch {
      // Sidecar may not exist
    }
  }

  async renameFile(oldPath: string, newPath: string): Promise<void> {
    await this.flushWrite(oldPath);

    const content = await this.readFile(oldPath);
    await this.writeFileImmediate(newPath, content);

    // Try to copy sidecar too
    try {
      const sidecarContent = await this.readFile(`${oldPath}.sidecar.json`);
      await this.writeFileImmediate(`${newPath}.sidecar.json`, sidecarContent);
    } catch {
      // No sidecar
    }

    await this.deleteFile(oldPath);
  }

  async fileExists(path: string): Promise<boolean> {
    if (!this.db) throw new Error('Database not initialized');

    const file = await this.db.get('files', path);
    return file !== undefined && !file.isDirectory;
  }

  async createFile(parentPath: string, name: string): Promise<FileNode> {
    const path = parentPath === '/' ? `/${name}` : `${parentPath}/${name}`;
    await this.writeFileImmediate(path, '');

    return {
      id: generateId(),
      name,
      path,
      type: 'file',
      category: this.categorizeFile(name),
    };
  }

  async createFolder(parentPath: string, name: string): Promise<FileNode> {
    const path = parentPath === '/' ? `/${name}` : `${parentPath}/${name}`;
    await this.ensureDirectoryPath(path);

    return {
      id: generateId(),
      name,
      path,
      type: 'directory',
    };
  }

  // Document operations

  async loadDocument(workspaceRoot: string, filePath: string): Promise<LoadedDocument> {
    if (!this.db) throw new Error('Database not initialized');

    let markdown: string;
    try {
      markdown = await this.readFile(filePath);
    } catch {
      markdown = '';
    }

    let sidecar: SidecarDocument;
    try {
      const sidecarJson = await this.readFile(`${filePath}.sidecar.json`);
      sidecar = JSON.parse(sidecarJson);
    } catch {
      sidecar = createEmptySidecar();
    }

    const docRecord = await this.db.getFromIndex('documents', 'by-path', filePath);
    let hasRecovery = false;
    let recoveryTime: string | undefined;

    if (docRecord) {
      const recovery = await this.db.get('recovery', docRecord.id);
      if (recovery) {
        hasRecovery = true;
        recoveryTime = recovery.timestamp;
      }
    }

    const json = await this.deserializer.deserialize(markdown, sidecar);

    return { json, sidecar, hasRecovery, recoveryTime };
  }

  async saveDocument(
    workspaceRoot: string,
    filePath: string,
    json: TiptapDocument,
    trigger: CheckpointTrigger
  ): Promise<SaveResult> {
    if (!this.db) throw new Error('Database not initialized');

    try {
      const { markdown, sidecar } = await this.serializer.serialize(json);

      await this.writeFileImmediate(filePath, markdown);
      await this.writeFileImmediate(`${filePath}.sidecar.json`, JSON.stringify(sidecar, null, 2));

      const contentHash = await this.storeObject(markdown);
      const sidecarHash = await this.storeObject(JSON.stringify(sidecar));

      let docRecord = await this.db.getFromIndex('documents', 'by-path', filePath);
      const now = new Date().toISOString();

      if (!docRecord) {
        docRecord = {
          id: generateId(),
          path: filePath,
          contentHash,
          sidecarHash,
          createdAt: now,
          updatedAt: now,
        };
        await this.db.put('documents', docRecord);
      } else {
        docRecord.contentHash = contentHash;
        docRecord.sidecarHash = sidecarHash;
        docRecord.updatedAt = now;
        await this.db.put('documents', docRecord);
      }

      const checkpoint: Checkpoint & { documentId: string } = {
        id: generateCheckpointId(),
        documentId: docRecord.id,
        contentHash,
        sidecarHash,
        timestamp: now,
        parentId: null,
        type: trigger === 'bookmark' ? 'bookmark' : 'auto',
        trigger,
        stats: {
          wordCount: sidecar.meta.wordCount || 0,
          charCount: markdown.length,
          changeSize: 0,
        },
      };

      await this.db.put('checkpoints', checkpoint);
      await this.db.delete('recovery', docRecord.id);

      return { success: true, checkpointId: checkpoint.id };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }

  // Workspace operations

  async initWorkspace(path: string): Promise<void> {
    await this.ensureDirectory('/');
  }

  async getCheckpoints(workspaceRoot: string, filePath: string): Promise<Checkpoint[]> {
    if (!this.db) throw new Error('Database not initialized');

    const docRecord = await this.db.getFromIndex('documents', 'by-path', filePath);
    if (!docRecord) return [];

    const checkpoints = await this.db.getAllFromIndex('checkpoints', 'by-document', docRecord.id);

    return checkpoints
      .map(({ documentId, ...cp }) => cp)
      .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
  }

  async restoreCheckpoint(
    workspaceRoot: string,
    filePath: string,
    checkpointId: string
  ): Promise<TiptapDocument> {
    if (!this.db) throw new Error('Database not initialized');

    const checkpoint = await this.db.get('checkpoints', checkpointId);
    if (!checkpoint) throw new Error('Checkpoint not found');

    const markdown = await this.loadObject(checkpoint.contentHash);
    const sidecarJson = await this.loadObject(checkpoint.sidecarHash);
    const sidecar = JSON.parse(sidecarJson);

    return this.deserializer.deserialize(markdown, sidecar);
  }

  // Object store

  private async storeObject(content: string): Promise<string> {
    if (!this.db) throw new Error('Database not initialized');

    const hash = await sha256(content);

    const existing = await this.db.get('objects', hash);
    if (existing) return hash;

    await this.db.put('objects', {
      hash,
      content,
      createdAt: new Date().toISOString(),
    });

    return hash;
  }

  private async loadObject(hash: string): Promise<string> {
    if (!this.db) throw new Error('Database not initialized');

    const obj = await this.db.get('objects', hash);
    if (!obj) throw new Error(`Object not found: ${hash}`);

    return obj.content;
  }

  // Image storage

  private async storeImage(hash: string, dataUrl: string): Promise<void> {
    if (!this.db) throw new Error('Database not initialized');

    const [header, base64Data] = dataUrl.split(',');
    const mimeMatch = header.match(/data:([^;]+)/);
    const mimeType = mimeMatch ? mimeMatch[1] : 'image/png';

    const binaryString = atob(base64Data);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }

    await this.db.put('images', {
      hash,
      data: bytes.buffer,
      mimeType,
      createdAt: new Date().toISOString(),
    });
  }

  private async loadImage(hash: string): Promise<string> {
    if (!this.db) throw new Error('Database not initialized');

    const image = await this.db.get('images', hash);
    if (!image) throw new Error(`Image not found: ${hash}`);

    const base64 = btoa(String.fromCharCode(...new Uint8Array(image.data)));
    return `data:${image.mimeType};base64,${base64}`;
  }
}
