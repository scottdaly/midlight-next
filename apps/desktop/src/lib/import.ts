// Import client - Tauri invoke wrappers for import/export operations

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { TiptapDocument } from '@midlight/core/types';

// ============================================================================
// Types
// ============================================================================

export type ImportSourceType = 'obsidian' | 'notion' | 'generic';

export type ImportFileType = 'markdown' | 'attachment' | 'other';

export interface ImportFileInfo {
  sourcePath: string;
  relativePath: string;
  name: string;
  fileType: ImportFileType;
  size: number;
  hasWikiLinks: boolean;
  hasFrontMatter: boolean;
  hasCallouts: boolean;
  hasDataview: boolean;
}

export interface AccessWarning {
  path: string;
  message: string;
}

export interface ImportAnalysis {
  sourceType: ImportSourceType;
  sourcePath: string;
  totalFiles: number;
  markdownFiles: number;
  attachments: number;
  folders: number;
  wikiLinks: number;
  filesWithWikiLinks: number;
  frontMatter: number;
  callouts: number;
  dataviewBlocks: number;
  csvDatabases: number;
  untitledPages: string[];
  emptyPages: string[];
  filesToImport: ImportFileInfo[];
  accessWarnings: AccessWarning[];
}

export interface ImportOptions {
  convertWikiLinks: boolean;
  importFrontMatter: boolean;
  convertCallouts: boolean;
  copyAttachments: boolean;
  preserveFolderStructure: boolean;
  skipEmptyPages: boolean;
  createMidlightFiles: boolean;
}

export type UntitledHandling = 'number' | 'keep' | 'prompt';

export interface NotionImportOptions extends ImportOptions {
  removeUuids: boolean;
  convertCsvToTables: boolean;
  untitledHandling: UntitledHandling;
}

export type ImportPhase = 'analyzing' | 'converting' | 'copying' | 'finalizing' | 'complete';

export interface ImportErrorInfo {
  file: string;
  message: string;
}

export interface ImportWarningInfo {
  file: string;
  message: string;
}

export interface ImportProgress {
  phase: ImportPhase;
  current: number;
  total: number;
  currentFile: string;
  errors: ImportErrorInfo[];
  warnings: ImportWarningInfo[];
}

export interface ImportResult {
  success: boolean;
  filesImported: number;
  linksConverted: number;
  attachmentsCopied: number;
  errors: ImportErrorInfo[];
  warnings: ImportWarningInfo[];
}

// ============================================================================
// DOCX Import Types
// ============================================================================

export interface DocxAnalysis {
  filePath: string;
  fileName: string;
  fileSize: number;
  paragraphCount: number;
  headingCount: number;
  imageCount: number;
  listCount: number;
  tableCount: number;
  hasImages: boolean;
  estimatedWords: number;
}

export interface ExtractedImage {
  id: string;
  data: number[]; // byte array
  contentType: string;
  originalName: string;
  relId: string;
}

export interface DocxImportWarning {
  warningType: string;
  message: string;
  details: string | null;
}

export interface DocxImportStats {
  paragraphs: number;
  headings: number;
  lists: number;
  images: number;
  tables: number;
  formattedRuns: number;
}

export interface DocxImportResult {
  tiptapJson: TiptapDocument;
  images: ExtractedImage[];
  warnings: DocxImportWarning[];
  stats: DocxImportStats;
}

// ============================================================================
// Default Options
// ============================================================================

export const defaultImportOptions: ImportOptions = {
  convertWikiLinks: true,
  importFrontMatter: true,
  convertCallouts: true,
  copyAttachments: true,
  preserveFolderStructure: true,
  skipEmptyPages: true,
  createMidlightFiles: true,
};

export const defaultNotionOptions: NotionImportOptions = {
  ...defaultImportOptions,
  removeUuids: true,
  convertCsvToTables: true,
  untitledHandling: 'number',
};

// ============================================================================
// Import Client
// ============================================================================

class ImportClient {
  /**
   * Open folder picker dialog
   */
  async selectFolder(): Promise<string | null> {
    return invoke<string | null>('import_select_folder');
  }

  /**
   * Detect the type of import source
   */
  async detectSourceType(folderPath: string): Promise<ImportSourceType> {
    return invoke<ImportSourceType>('import_detect_source_type', { folderPath });
  }

  /**
   * Analyze an Obsidian vault
   */
  async analyzeObsidian(vaultPath: string): Promise<ImportAnalysis> {
    return invoke<ImportAnalysis>('import_analyze_obsidian', { vaultPath });
  }

  /**
   * Analyze a Notion export
   */
  async analyzeNotion(exportPath: string): Promise<ImportAnalysis> {
    return invoke<ImportAnalysis>('import_analyze_notion', { exportPath });
  }

  /**
   * Import an Obsidian vault
   */
  async importObsidian(
    analysis: ImportAnalysis,
    destPath: string,
    options: ImportOptions
  ): Promise<ImportResult> {
    return invoke<ImportResult>('import_obsidian', {
      analysisJson: JSON.stringify(analysis),
      destPath,
      optionsJson: JSON.stringify(options),
    });
  }

  /**
   * Import a Notion export
   */
  async importNotion(
    analysis: ImportAnalysis,
    destPath: string,
    options: NotionImportOptions
  ): Promise<ImportResult> {
    return invoke<ImportResult>('import_notion', {
      analysisJson: JSON.stringify(analysis),
      destPath,
      optionsJson: JSON.stringify(options),
    });
  }

  /**
   * Cancel an active import
   */
  async cancel(): Promise<void> {
    return invoke('import_cancel');
  }

  /**
   * Export current document to PDF
   */
  async exportPdf(): Promise<boolean> {
    return invoke<boolean>('export_pdf');
  }

  /**
   * Listen for import progress events
   */
  onProgress(callback: (progress: ImportProgress) => void): Promise<UnlistenFn> {
    return listen<ImportProgress>('import-progress', (event) => {
      callback(event.payload);
    });
  }

  // ==========================================================================
  // DOCX Import Methods
  // ==========================================================================

  /**
   * Open file picker dialog for DOCX files
   */
  async selectDocxFile(): Promise<string | null> {
    return invoke<string | null>('import_select_docx_file');
  }

  /**
   * Analyze a DOCX file without importing
   */
  async analyzeDocx(filePath: string): Promise<DocxAnalysis> {
    return invoke<DocxAnalysis>('import_analyze_docx', { filePath });
  }

  /**
   * Import a DOCX file and return Tiptap JSON
   */
  async importDocx(
    filePath: string,
    workspaceRoot: string,
    destFilename?: string
  ): Promise<DocxImportResult> {
    return invoke<DocxImportResult>('import_docx_file', {
      filePath,
      workspaceRoot,
      destFilename,
    });
  }

  /**
   * Listen for DOCX import completion events
   */
  onDocxComplete(
    callback: (data: { baseName: string; imageCount: number; warningCount: number }) => void
  ): Promise<UnlistenFn> {
    return listen<{ baseName: string; imageCount: number; warningCount: number }>(
      'import-docx-complete',
      (event) => {
        callback(event.payload);
      }
    );
  }
}

export const importClient = new ImportClient();
