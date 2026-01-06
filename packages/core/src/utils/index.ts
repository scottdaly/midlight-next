// @midlight/core/utils - Utility functions

/**
 * Generates a secure random ID for blocks
 * Uses crypto.getRandomValues which works in both Node.js and browsers
 */
export function generateBlockId(prefix = 'blk'): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  const hex = Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
  return `${prefix}-${hex}`;
}

/**
 * Generates a secure random ID (generic version)
 */
export function generateId(length = 16): string {
  const bytes = new Uint8Array(Math.ceil(length / 2));
  crypto.getRandomValues(bytes);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
    .slice(0, length);
}

/**
 * Generates a checkpoint ID
 */
export function generateCheckpointId(): string {
  return `cp-${generateId(8)}`;
}

/**
 * Converts a file path to a safe key for storage
 * Replaces path separators with double underscores
 */
export function pathToKey(path: string): string {
  return path.replace(/[/\\]/g, '__').replace(/\./g, '_');
}

/**
 * Converts a storage key back to a file path
 */
export function keyToPath(key: string): string {
  return key.replace(/__/g, '/').replace(/_(?!_)/g, '.');
}

/**
 * Calculates SHA-256 hash of content
 * Works in both Node.js (via crypto) and browsers (via SubtleCrypto)
 */
export async function sha256(content: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(content);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Debounce function for rate-limiting
 */
export function debounce<T extends (...args: unknown[]) => unknown>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return (...args: Parameters<T>) => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => {
      fn(...args);
      timeoutId = null;
    }, delay);
  };
}

/**
 * Throttle function for rate-limiting
 */
export function throttle<T extends (...args: unknown[]) => unknown>(
  fn: T,
  limit: number
): (...args: Parameters<T>) => void {
  let lastCall = 0;

  return (...args: Parameters<T>) => {
    const now = Date.now();
    if (now - lastCall >= limit) {
      lastCall = now;
      fn(...args);
    }
  };
}

/**
 * Deep clone an object (JSON-safe)
 */
export function deepClone<T>(obj: T): T {
  return JSON.parse(JSON.stringify(obj));
}

/**
 * Sleep for a specified duration
 */
export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Sorts file nodes: directories first, then alphabetically
 */
export function sortFileNodes<T extends { type: 'file' | 'directory'; name: string }>(
  nodes: T[]
): T[] {
  return [...nodes].sort((a, b) => {
    // Directories first
    if (a.type === 'directory' && b.type === 'file') return -1;
    if (a.type === 'file' && b.type === 'directory') return 1;
    // Then alphabetically (case-insensitive)
    return a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  });
}

/**
 * Gets the file extension from a path
 */
export function getExtension(path: string): string {
  const lastDot = path.lastIndexOf('.');
  const lastSlash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (lastDot > lastSlash) {
    return path.slice(lastDot + 1).toLowerCase();
  }
  return '';
}

/**
 * Gets the filename from a path
 */
export function getFilename(path: string): string {
  const lastSlash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return path.slice(lastSlash + 1);
}

/**
 * Gets the directory from a path
 */
export function getDirectory(path: string): string {
  const lastSlash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (lastSlash === -1) return '';
  return path.slice(0, lastSlash);
}

/**
 * Joins path segments
 */
export function joinPath(...segments: string[]): string {
  return segments
    .map((s, i) => {
      if (i === 0) return s.replace(/[/\\]+$/, '');
      return s.replace(/^[/\\]+|[/\\]+$/g, '');
    })
    .filter((s) => s.length > 0)
    .join('/');
}

/**
 * Normalizes a path (removes double slashes, etc.)
 */
export function normalizePath(path: string): string {
  return path.replace(/[/\\]+/g, '/').replace(/\/$/, '');
}

/**
 * Checks if a path is absolute
 */
export function isAbsolutePath(path: string): boolean {
  // Unix absolute path
  if (path.startsWith('/')) return true;
  // Windows absolute path (C:\, D:\, etc.)
  if (/^[a-zA-Z]:[/\\]/.test(path)) return true;
  return false;
}

/**
 * Validates that a relative path doesn't escape the root
 * (prevents directory traversal attacks)
 */
export function isPathSafe(relativePath: string): boolean {
  const normalized = normalizePath(relativePath);
  const parts = normalized.split('/');

  let depth = 0;
  for (const part of parts) {
    if (part === '..') {
      depth--;
      if (depth < 0) return false;
    } else if (part !== '.' && part !== '') {
      depth++;
    }
  }

  return true;
}

/**
 * Formats a date as ISO string
 */
export function formatDate(date: Date | string | number): string {
  const d = typeof date === 'string' || typeof date === 'number' ? new Date(date) : date;
  return d.toISOString();
}

/**
 * Formats bytes as human-readable string
 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

/**
 * Creates an empty MidlightDocument with default settings
 */
export function createEmptyMidlightDocument(options?: {
  title?: string;
  defaultFont?: string;
  defaultFontSize?: number;
}): import('../types/index.js').MidlightDocument {
  const now = new Date().toISOString();
  return {
    version: 1,
    meta: {
      created: now,
      modified: now,
      title: options?.title,
    },
    document: {
      defaultFont: options?.defaultFont ?? 'Merriweather',
      defaultFontSize: options?.defaultFontSize ?? 16,
    },
    content: {
      type: 'doc',
      content: [{ type: 'paragraph' }],
    },
  };
}

/**
 * Determines the file category based on extension
 */
export function getFileCategory(filename: string): import('../types/index.js').FileCategory {
  const ext = getExtension(filename).toLowerCase();

  // Native formats
  if (ext === 'midlight') return 'midlight';
  if (ext === 'md') return 'native';

  // Compatible formats
  if (['txt', 'json'].includes(ext)) return 'compatible';

  // Importable formats
  if (ext === 'docx') return 'importable';

  // Viewable formats
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'pdf'].includes(ext)) return 'viewable';

  return 'unsupported';
}

/**
 * Checks if a file should be shown in the file tree
 */
export function shouldShowInFileTree(filename: string): boolean {
  // Hide hidden files (starting with .)
  if (filename.startsWith('.')) return false;

  // Hide backup files
  if (filename.endsWith('.backup')) return false;

  // Hide sidecar files
  if (filename.endsWith('.sidecar.json')) return false;

  return true;
}
