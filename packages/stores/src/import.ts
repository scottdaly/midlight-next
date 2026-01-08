// @midlight/stores/import - Import/export state management

import { writable, derived } from 'svelte/store';

// ============================================================================
// Types (duplicated from import client for store-only usage)
// ============================================================================

export type ImportSourceType = 'obsidian' | 'notion' | 'generic';
export type ImportPhase = 'analyzing' | 'converting' | 'copying' | 'finalizing' | 'complete';
export type ImportStep = 'select' | 'analyze' | 'options' | 'importing' | 'complete';

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

export interface CurrentImport {
  sourcePath: string;
  sourceType: ImportSourceType;
  progress: ImportProgress | null;
}

export interface ImportState {
  isImporting: boolean;
  currentImport: CurrentImport | null;
  lastResult: ImportResult | null;
}

// ============================================================================
// Export State
// ============================================================================

export type ExportType = 'pdf' | 'docx';

export interface ExportProgress {
  phase: string;
  current: number;
  total: number;
}

export interface ExportState {
  isExporting: boolean;
  exportType: ExportType | null;
  progress: ExportProgress | null;
  error: string | null;
}

// ============================================================================
// Import Store
// ============================================================================

const initialImportState: ImportState = {
  isImporting: false,
  currentImport: null,
  lastResult: null,
};

function createImportStore() {
  const { subscribe, set, update } = writable<ImportState>(initialImportState);

  return {
    subscribe,

    /**
     * Start an import operation
     */
    startImport(sourcePath: string, sourceType: ImportSourceType) {
      update((s) => ({
        ...s,
        isImporting: true,
        currentImport: {
          sourcePath,
          sourceType,
          progress: null,
        },
        lastResult: null,
      }));
    },

    /**
     * Update import progress
     */
    updateProgress(progress: ImportProgress) {
      update((s) => {
        if (!s.currentImport) return s;
        return {
          ...s,
          currentImport: {
            ...s.currentImport,
            progress,
          },
        };
      });
    },

    /**
     * Complete the import
     */
    completeImport(result: ImportResult) {
      update((s) => ({
        ...s,
        isImporting: false,
        currentImport: null,
        lastResult: result,
      }));
    },

    /**
     * Cancel the import
     */
    cancelImport() {
      update((s) => ({
        ...s,
        isImporting: false,
        currentImport: null,
      }));
    },

    /**
     * Clear last result
     */
    clearResult() {
      update((s) => ({
        ...s,
        lastResult: null,
      }));
    },

    /**
     * Reset the store
     */
    reset() {
      set(initialImportState);
    },
  };
}

export const importStore = createImportStore();

// Derived stores
export const isImporting = derived(importStore, ($store) => $store.isImporting);
export const currentImport = derived(importStore, ($store) => $store.currentImport);
export const importProgress = derived(
  importStore,
  ($store) => $store.currentImport?.progress ?? null
);

// ============================================================================
// Export Store
// ============================================================================

const initialExportState: ExportState = {
  isExporting: false,
  exportType: null,
  progress: null,
  error: null,
};

function createExportStore() {
  const { subscribe, set, update } = writable<ExportState>(initialExportState);

  return {
    subscribe,

    /**
     * Start an export operation
     */
    startExport(type: ExportType) {
      update((s) => ({
        ...s,
        isExporting: true,
        exportType: type,
        progress: null,
        error: null,
      }));
    },

    /**
     * Update export progress
     */
    updateProgress(progress: ExportProgress) {
      update((s) => ({
        ...s,
        progress,
      }));
    },

    /**
     * Complete the export
     */
    completeExport() {
      update((s) => ({
        ...s,
        isExporting: false,
        exportType: null,
        progress: null,
      }));
    },

    /**
     * Fail the export
     */
    failExport(error: string) {
      update((s) => ({
        ...s,
        isExporting: false,
        error,
      }));
    },

    /**
     * Clear error
     */
    clearError() {
      update((s) => ({
        ...s,
        error: null,
      }));
    },

    /**
     * Reset the store
     */
    reset() {
      set(initialExportState);
    },
  };
}

export const exportStore = createExportStore();

// Derived stores
export const isExporting = derived(exportStore, ($store) => $store.isExporting);
export const exportProgress = derived(exportStore, ($store) => $store.progress);
export const exportError = derived(exportStore, ($store) => $store.error);
