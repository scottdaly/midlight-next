// Export client for Tauri backend
// Handles DOCX and PDF export operations

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ============================================================================
// Types
// ============================================================================

export type ExportType = 'pdf' | 'docx';

export interface ExportProgress {
  current: number;
  total: number;
  phase: string;
}

export interface ExportResult {
  success: boolean;
  path: string | null;
  error: string | null;
}

export interface TiptapDocument {
  type: 'doc';
  content: TiptapNode[];
}

export interface TiptapNode {
  type: string;
  content?: TiptapNode[];
  text?: string;
  marks?: TiptapMark[];
  attrs?: Record<string, unknown>;
}

export interface TiptapMark {
  type: string;
  attrs?: Record<string, unknown>;
}

// ============================================================================
// Export Client
// ============================================================================

class ExportClient {
  /**
   * Opens a save dialog for selecting the export destination
   */
  async selectSavePath(defaultName: string, fileType: ExportType): Promise<string | null> {
    return invoke<string | null>('export_select_save_path', {
      defaultName,
      fileType,
    });
  }

  /**
   * Exports the document to DOCX format
   */
  async exportToDocx(content: TiptapDocument, outputPath: string): Promise<ExportResult> {
    return invoke<ExportResult>('export_to_docx', {
      content,
      outputPath,
    });
  }

  /**
   * Exports the document to PDF using the system print dialog
   * This uses the webview's native print functionality
   */
  async exportToPdf(): Promise<boolean> {
    return invoke<boolean>('export_pdf');
  }

  /**
   * Listens for export progress events
   */
  async onProgress(callback: (progress: ExportProgress) => void): Promise<UnlistenFn> {
    return listen<ExportProgress>('export:progress', (event) => {
      callback(event.payload);
    });
  }

  /**
   * High-level export function that handles the full export flow
   */
  async export(
    content: TiptapDocument,
    documentName: string,
    exportType: ExportType,
    onProgress?: (progress: ExportProgress) => void
  ): Promise<ExportResult> {
    // For PDF, use the print dialog
    if (exportType === 'pdf') {
      try {
        await this.exportToPdf();
        return {
          success: true,
          path: null,
          error: null,
        };
      } catch (e) {
        return {
          success: false,
          path: null,
          error: e instanceof Error ? e.message : String(e),
        };
      }
    }

    // For DOCX, use the save dialog and export
    const outputPath = await this.selectSavePath(documentName, exportType);
    if (!outputPath) {
      return {
        success: false,
        path: null,
        error: 'Export cancelled',
      };
    }

    // Set up progress listener if provided
    let unlisten: UnlistenFn | undefined;
    if (onProgress) {
      unlisten = await this.onProgress(onProgress);
    }

    try {
      const result = await this.exportToDocx(content, outputPath);
      return result;
    } finally {
      if (unlisten) {
        unlisten();
      }
    }
  }
}

export const exportClient = new ExportClient();
