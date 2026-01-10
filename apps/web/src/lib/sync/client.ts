// Sync Client - API client for cloud document synchronization

import { get } from 'svelte/store';
import { auth } from '@midlight/stores';

const API_BASE = import.meta.env.VITE_API_URL || 'https://midlight.ai';

// Request timeout in milliseconds
const DEFAULT_TIMEOUT_MS = 30000; // 30 seconds
const UPLOAD_TIMEOUT_MS = 60000; // 60 seconds for uploads

export interface SyncDocument {
  id: string;
  path: string;
  contentHash: string;
  sidecarHash: string;
  version: number;
  sizeBytes: number;
  updatedAt: string;
  deleted?: boolean;
}

export interface SyncConflict {
  id: string;
  documentId: string;
  path: string;
  localVersion: number;
  remoteVersion: number;
  createdAt: string;
}

export interface SyncUsage {
  documentCount: number;
  totalSizeBytes: number;
  limitBytes: number;
  remainingBytes: number;
  percentUsed: number;
  lastSyncAt: string | null;
  tier: 'free' | 'premium' | 'pro';
}

export interface SyncStatus {
  documents: SyncDocument[];
  usage: {
    documentCount: number;
    totalSizeBytes: number;
    limitBytes: number;
    percentUsed: number;
    lastSyncAt: string | null;
  };
  conflicts: SyncConflict[];
  storageAvailable: boolean;
}

export interface DocumentContent {
  id: string;
  path: string;
  content: string;
  sidecar: Record<string, unknown>;
  contentHash: string;
  sidecarHash: string;
  version: number;
  updatedAt: string;
}

export interface ConflictDetails {
  id: string;
  documentId: string;
  path: string;
  local: {
    version: number;
    content: string;
    sidecar: Record<string, unknown>;
  } | null;
  remote: {
    version: number;
    content: string;
    sidecar: Record<string, unknown>;
  } | null;
  createdAt: string;
  resolved: boolean;
}

export interface SyncResult {
  success: boolean;
  document?: SyncDocument;
  conflict?: {
    id: string;
    documentId: string;
    localVersion: number;
    remoteVersion: number;
    remoteContent: string;
    remoteSidecar: Record<string, unknown>;
  };
  error?: string;
}

export type ConflictResolution = 'local' | 'remote' | 'both';

class SyncClient {
  private accessToken: string | null = null;

  /**
   * Set the access token for API requests
   */
  setAccessToken(token: string | null) {
    this.accessToken = token;
  }

  /**
   * Get headers for API requests
   */
  private getHeaders(): HeadersInit {
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      'X-Client-Type': 'web',
    };

    if (this.accessToken) {
      headers['Authorization'] = `Bearer ${this.accessToken}`;
    }

    return headers;
  }

  /**
   * Make an API request with error handling and timeout
   */
  private async request<T>(
    path: string,
    options: RequestInit = {},
    timeoutMs: number = DEFAULT_TIMEOUT_MS
  ): Promise<T> {
    const authState = get(auth);
    if (!authState.isAuthenticated) {
      throw new Error('Not authenticated');
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

    try {
      const response = await fetch(`${API_BASE}${path}`, {
        ...options,
        headers: {
          ...this.getHeaders(),
          ...options.headers,
        },
        credentials: 'include',
        signal: controller.signal,
      });

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: 'Request failed' }));
        throw new Error(error.error || `HTTP ${response.status}`);
      }

      return response.json();
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        throw new Error(`Request timeout after ${timeoutMs}ms`);
      }
      throw error;
    } finally {
      clearTimeout(timeoutId);
    }
  }

  /**
   * Get sync status - all documents, conflicts, and usage
   */
  async getStatus(): Promise<SyncStatus> {
    return this.request<SyncStatus>('/api/sync/status');
  }

  /**
   * Upload/sync a document
   */
  async uploadDocument(
    path: string,
    content: string,
    sidecar: Record<string, unknown>,
    baseVersion?: number
  ): Promise<SyncResult> {
    try {
      const result = await this.request<{ success: boolean; document: SyncDocument }>(
        '/api/sync/documents',
        {
          method: 'POST',
          body: JSON.stringify({ path, content, sidecar, baseVersion }),
        },
        UPLOAD_TIMEOUT_MS // Use longer timeout for uploads
      );
      return { success: true, document: result.document };
    } catch (error) {
      // Check for conflict response (409)
      if (error instanceof Error && error.message.includes('409')) {
        // Re-fetch to get conflict details
        const status = await this.getStatus();
        const conflict = status.conflicts.find(c => c.path === path);
        if (conflict) {
          const details = await this.getConflictDetails(conflict.id);
          return {
            success: false,
            conflict: {
              id: conflict.id,
              documentId: conflict.documentId,
              localVersion: conflict.localVersion,
              remoteVersion: conflict.remoteVersion,
              remoteContent: details.remote?.content || '',
              remoteSidecar: details.remote?.sidecar || {},
            },
          };
        }
      }
      return { success: false, error: error instanceof Error ? error.message : 'Unknown error' };
    }
  }

  /**
   * Download a specific document
   */
  async downloadDocument(documentId: string): Promise<DocumentContent> {
    return this.request<DocumentContent>(`/api/sync/documents/${documentId}`);
  }

  /**
   * Delete a document (soft delete)
   */
  async deleteDocument(documentId: string): Promise<{ success: boolean; deletedAt: string }> {
    return this.request(`/api/sync/documents/${documentId}`, {
      method: 'DELETE',
    });
  }

  /**
   * Get conflict details with both versions
   */
  async getConflictDetails(conflictId: string): Promise<ConflictDetails> {
    return this.request<ConflictDetails>(`/api/sync/conflicts/${conflictId}`);
  }

  /**
   * Resolve a conflict
   */
  async resolveConflict(
    conflictId: string,
    resolution: ConflictResolution
  ): Promise<{ success: boolean; resolution: string; resolvedAt: string }> {
    return this.request(`/api/sync/conflicts/${conflictId}/resolve`, {
      method: 'POST',
      body: JSON.stringify({ resolution }),
    });
  }

  /**
   * Get storage usage
   */
  async getUsage(): Promise<SyncUsage> {
    return this.request<SyncUsage>('/api/sync/usage');
  }

  /**
   * Calculate content hash (must match server's hashContent)
   */
  async hashContent(content: string): Promise<string> {
    const encoder = new TextEncoder();
    const data = encoder.encode(content);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
  }

  /**
   * Compare local and remote hashes to detect changes
   */
  async hasLocalChanges(
    localContent: string,
    remoteHash: string
  ): Promise<boolean> {
    const localHash = await this.hashContent(localContent);
    return localHash !== remoteHash;
  }
}

// Singleton instance
export const syncClient = new SyncClient();
