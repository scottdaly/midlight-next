// Web Storage Adapter - OPFS + IndexedDB implementation
// Provides the same interface as Tauri for desktop, enabling code sharing

// Type augmentation for FileSystemDirectoryHandle.entries()
declare global {
  interface FileSystemDirectoryHandle {
    entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
  }
}

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

interface MidlightDB {
  workspaces: {
    key: string;
    value: {
      id: string;
      name: string;
      createdAt: string;
      lastOpenedAt: string;
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

export class WebStorageAdapter implements StorageAdapter {
  private opfsRoot: FileSystemDirectoryHandle | null = null;
  private db: IDBPDatabase<MidlightDB> | null = null;
  private serializer: DocumentSerializer;
  private deserializer: DocumentDeserializer;

  constructor() {
    this.serializer = new DocumentSerializer({
      storeImage: async (dataUrl) => {
        // Store image in OPFS and return reference
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
    // Initialize OPFS
    if ('storage' in navigator && 'getDirectory' in navigator.storage) {
      this.opfsRoot = await navigator.storage.getDirectory();
    } else {
      throw new Error('Origin Private File System (OPFS) not supported');
    }

    // Initialize IndexedDB
    this.db = await openDB<MidlightDB>('midlight', 1, {
      upgrade(db) {
        // Workspaces store
        if (!db.objectStoreNames.contains('workspaces')) {
          db.createObjectStore('workspaces', { keyPath: 'id' });
        }

        // Documents store
        if (!db.objectStoreNames.contains('documents')) {
          const docStore = db.createObjectStore('documents', { keyPath: 'id' });
          docStore.createIndex('by-path', 'path', { unique: true });
        }

        // Objects store (content-addressable)
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

    // Ensure documents directory exists in OPFS
    await this.ensureDirectory('documents');
    await this.ensureDirectory('images');
  }

  private async ensureDirectory(name: string): Promise<FileSystemDirectoryHandle> {
    if (!this.opfsRoot) throw new Error('OPFS not initialized');
    return this.opfsRoot.getDirectoryHandle(name, { create: true });
  }

  private async getDocumentsDir(): Promise<FileSystemDirectoryHandle> {
    return this.ensureDirectory('documents');
  }

  private async getImagesDir(): Promise<FileSystemDirectoryHandle> {
    return this.ensureDirectory('images');
  }

  // File operations

  async readDir(path: string): Promise<FileNode[]> {
    const docsDir = await this.getDocumentsDir();
    const entries: FileNode[] = [];

    // If path is root, list all files in documents dir
    const targetDir = path === '/' ? docsDir : await this.getSubdirectory(docsDir, path);

    for await (const [name, handle] of targetDir.entries()) {
      // Skip sidecar files in listing
      if (name.endsWith('.sidecar.json')) continue;

      const isDir = handle.kind === 'directory';
      const filePath = path === '/' ? `/${name}` : `${path}/${name}`;

      entries.push({
        id: generateId(),
        name,
        path: filePath,
        type: isDir ? 'directory' : 'file',
        category: isDir ? undefined : this.categorizeFile(name),
      });
    }

    return sortFileNodes(entries);
  }

  private async getSubdirectory(
    root: FileSystemDirectoryHandle,
    path: string
  ): Promise<FileSystemDirectoryHandle> {
    const parts = path.split('/').filter(Boolean);
    let current = root;

    for (const part of parts) {
      current = await current.getDirectoryHandle(part);
    }

    return current;
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

  async readFile(path: string): Promise<string> {
    const docsDir = await this.getDocumentsDir();
    const parts = path.split('/').filter(Boolean);
    const fileName = parts.pop()!;

    let dir = docsDir;
    for (const part of parts) {
      dir = await dir.getDirectoryHandle(part);
    }

    const fileHandle = await dir.getFileHandle(fileName);
    const file = await fileHandle.getFile();
    return file.text();
  }

  async writeFile(path: string, content: string): Promise<void> {
    const docsDir = await this.getDocumentsDir();
    const parts = path.split('/').filter(Boolean);
    const fileName = parts.pop()!;

    let dir = docsDir;
    for (const part of parts) {
      dir = await dir.getDirectoryHandle(part, { create: true });
    }

    const fileHandle = await dir.getFileHandle(fileName, { create: true });
    const writable = await fileHandle.createWritable();
    await writable.write(content);
    await writable.close();
  }

  async deleteFile(path: string): Promise<void> {
    const docsDir = await this.getDocumentsDir();
    const parts = path.split('/').filter(Boolean);
    const fileName = parts.pop()!;

    let dir = docsDir;
    for (const part of parts) {
      dir = await dir.getDirectoryHandle(part);
    }

    await dir.removeEntry(fileName);

    // Also delete sidecar if exists
    try {
      await dir.removeEntry(`${fileName}.sidecar.json`);
    } catch {
      // Sidecar may not exist
    }
  }

  async renameFile(oldPath: string, newPath: string): Promise<void> {
    // OPFS doesn't have native rename, so copy + delete
    const content = await this.readFile(oldPath);
    await this.writeFile(newPath, content);

    // Try to copy sidecar too
    try {
      const sidecarContent = await this.readFile(`${oldPath}.sidecar.json`);
      await this.writeFile(`${newPath}.sidecar.json`, sidecarContent);
    } catch {
      // No sidecar
    }

    await this.deleteFile(oldPath);
  }

  async fileExists(path: string): Promise<boolean> {
    try {
      await this.readFile(path);
      return true;
    } catch {
      return false;
    }
  }

  // Document operations

  async loadDocument(workspaceRoot: string, filePath: string): Promise<LoadedDocument> {
    if (!this.db) throw new Error('Database not initialized');

    // Read markdown file
    let markdown: string;
    try {
      markdown = await this.readFile(filePath);
    } catch {
      // New file
      markdown = '';
    }

    // Read sidecar
    let sidecar: SidecarDocument;
    try {
      const sidecarJson = await this.readFile(`${filePath}.sidecar.json`);
      sidecar = JSON.parse(sidecarJson);
    } catch {
      sidecar = createEmptySidecar();
    }

    // Check for recovery
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

    // Deserialize to Tiptap JSON
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
      // Serialize to markdown + sidecar
      const { markdown, sidecar } = await this.serializer.serialize(json);

      // Write files
      await this.writeFile(filePath, markdown);
      await this.writeFile(`${filePath}.sidecar.json`, JSON.stringify(sidecar, null, 2));

      // Store in object store (for versioning)
      const contentHash = await this.storeObject(markdown);
      const sidecarHash = await this.storeObject(JSON.stringify(sidecar));

      // Get or create document record
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

      // Create checkpoint
      const checkpoint: Checkpoint & { documentId: string } = {
        id: generateCheckpointId(),
        documentId: docRecord.id,
        contentHash,
        sidecarHash,
        timestamp: now,
        parentId: null, // TODO: Track parent
        type: trigger === 'bookmark' ? 'bookmark' : 'auto',
        trigger,
        stats: {
          wordCount: sidecar.meta.wordCount || 0,
          charCount: markdown.length,
          changeSize: 0, // TODO: Calculate
        },
      };

      await this.db.put('checkpoints', checkpoint);

      // Clear recovery
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
    // For web, workspace is just the OPFS documents folder
    await this.ensureDirectory('documents');
    await this.ensureDirectory('images');
  }

  async getCheckpoints(workspaceRoot: string, filePath: string): Promise<Checkpoint[]> {
    if (!this.db) throw new Error('Database not initialized');

    const docRecord = await this.db.getFromIndex('documents', 'by-path', filePath);
    if (!docRecord) return [];

    const checkpoints = await this.db.getAllFromIndex(
      'checkpoints',
      'by-document',
      docRecord.id
    );

    // Sort by timestamp descending
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

  // Object store (content-addressable)

  private async storeObject(content: string): Promise<string> {
    if (!this.db) throw new Error('Database not initialized');

    const hash = await sha256(content);

    // Check if already exists
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
    const imagesDir = await this.getImagesDir();

    // Parse data URL
    const [header, base64Data] = dataUrl.split(',');
    const mimeMatch = header.match(/data:([^;]+)/);
    const mimeType = mimeMatch ? mimeMatch[1] : 'image/png';
    const ext = mimeType.split('/')[1] || 'png';

    // Convert to binary
    const binaryString = atob(base64Data);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }

    // Write to OPFS
    const fileHandle = await imagesDir.getFileHandle(`${hash}.${ext}`, { create: true });
    const writable = await fileHandle.createWritable();
    await writable.write(bytes);
    await writable.close();
  }

  private async loadImage(hash: string): Promise<string> {
    const imagesDir = await this.getImagesDir();

    // Try common extensions
    const extensions = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'];

    for (const ext of extensions) {
      try {
        const fileHandle = await imagesDir.getFileHandle(`${hash}.${ext}`);
        const file = await fileHandle.getFile();
        const buffer = await file.arrayBuffer();
        const base64 = btoa(String.fromCharCode(...new Uint8Array(buffer)));
        return `data:image/${ext};base64,${base64}`;
      } catch {
        // Try next extension
      }
    }

    throw new Error(`Image not found: ${hash}`);
  }

  // Create new file

  async createFile(parentPath: string, name: string): Promise<FileNode> {
    const path = parentPath === '/' ? `/${name}` : `${parentPath}/${name}`;
    await this.writeFile(path, '');

    return {
      id: generateId(),
      name,
      path,
      type: 'file',
      category: this.categorizeFile(name),
    };
  }

  // Create new folder

  async createFolder(parentPath: string, name: string): Promise<FileNode> {
    const path = parentPath === '/' ? `/${name}` : `${parentPath}/${name}`;
    const docsDir = await this.getDocumentsDir();
    const parts = path.split('/').filter(Boolean);

    let dir = docsDir;
    for (const part of parts) {
      dir = await dir.getDirectoryHandle(part, { create: true });
    }

    return {
      id: generateId(),
      name,
      path,
      type: 'directory',
    };
  }
}
