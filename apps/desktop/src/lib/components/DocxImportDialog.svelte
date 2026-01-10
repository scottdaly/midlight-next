<script lang="ts">
  import {
    importClient,
    type DocxAnalysis,
    type DocxImportResult,
  } from '$lib/import';

  interface Props {
    open: boolean;
    onClose: () => void;
    workspaceRoot: string;
    onComplete?: (result: DocxImportResult, fileName: string) => void;
  }

  let { open, onClose, workspaceRoot, onComplete }: Props = $props();

  type ImportStep = 'select' | 'analyze' | 'importing' | 'complete';

  // State
  let step: ImportStep = $state('select');
  let filePath: string | null = $state(null);
  let analysis: DocxAnalysis | null = $state(null);
  let result: DocxImportResult | null = $state(null);
  let error: string | null = $state(null);
  let isAnalyzing: boolean = $state(false);
  let isImporting: boolean = $state(false);

  // Reset state when dialog closes
  $effect(() => {
    if (!open) {
      step = 'select';
      filePath = null;
      analysis = null;
      result = null;
      error = null;
      isAnalyzing = false;
      isImporting = false;
    }
  });

  // Select DOCX file
  async function handleSelectFile() {
    try {
      const selected = await importClient.selectDocxFile();
      if (selected) {
        filePath = selected;
        await handleAnalyze();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  // Analyze the DOCX file
  async function handleAnalyze() {
    if (!filePath) return;

    isAnalyzing = true;
    error = null;

    try {
      analysis = await importClient.analyzeDocx(filePath);
      step = 'analyze';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      isAnalyzing = false;
    }
  }

  // Import the DOCX file
  async function handleImport() {
    if (!filePath || !analysis) return;

    isImporting = true;
    error = null;
    step = 'importing';

    try {
      result = await importClient.importDocx(filePath, workspaceRoot);
      step = 'complete';

      if (onComplete) {
        onComplete(result, analysis.fileName);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      step = 'analyze';
    } finally {
      isImporting = false;
    }
  }

  // Format file size
  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  // Get filename from path
  function getFileName(path: string): string {
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  // Keyboard handling
  function handleKeyDown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape' && step !== 'importing') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open}
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={() => step !== 'importing' && onClose()}
    role="presentation"
  >
    <div
      class="bg-card border border-border rounded-lg shadow-xl max-w-lg w-full mx-4 max-h-[80vh] flex flex-col"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="docx-import-title"
    >
      <!-- Header -->
      <div class="px-6 py-4 border-b border-border">
        <h2 id="docx-import-title" class="text-lg font-semibold text-foreground">
          {#if step === 'select'}
            Import Word Document
          {:else if step === 'analyze'}
            Import "{analysis?.fileName || 'Document'}"
          {:else if step === 'importing'}
            Importing...
          {:else}
            Import Complete
          {/if}
        </h2>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto px-6 py-4">
        {#if error}
          <div class="mb-4 p-3 bg-destructive/10 border border-destructive/20 rounded text-destructive text-sm">
            {error}
          </div>
        {/if}

        {#if step === 'select'}
          <!-- Select Step -->
          <div class="space-y-4">
            <p class="text-muted-foreground text-sm">
              Import a Microsoft Word document (.docx) and convert it to a Midlight document with formatting preserved.
            </p>

            <button
              class="w-full p-8 border-2 border-dashed border-border rounded-lg hover:border-primary/50 hover:bg-accent/50 transition-colors text-center"
              onclick={handleSelectFile}
              disabled={isAnalyzing}
            >
              {#if isAnalyzing}
                <div class="flex items-center justify-center gap-2">
                  <svg class="animate-spin h-5 w-5 text-muted-foreground" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
                  </svg>
                  <span class="text-muted-foreground">Analyzing...</span>
                </div>
              {:else}
                <div class="text-muted-foreground">
                  <svg class="mx-auto h-12 w-12 mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                  </svg>
                  <p class="font-medium text-foreground">Choose a Word document</p>
                  <p class="text-sm mt-1">Click to browse for .docx files</p>
                </div>
              {/if}
            </button>

            <div class="text-xs text-muted-foreground space-y-1">
              <p><strong>Supported:</strong></p>
              <ul class="list-disc list-inside space-y-0.5 ml-2">
                <li>Paragraphs and headings (H1-H6)</li>
                <li>Bold, italic, underline, strikethrough</li>
                <li>Bullet and numbered lists</li>
                <li>Text colors and highlights</li>
                <li>Embedded images</li>
                <li>Font sizes and families</li>
              </ul>
            </div>
          </div>

        {:else if step === 'analyze' && analysis}
          <!-- Analysis Step -->
          <div class="space-y-4">
            <div class="p-3 bg-accent/50 rounded-lg">
              <div class="flex items-center gap-2 mb-2">
                <svg class="h-5 w-5 text-blue-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                <span class="font-medium text-foreground">{analysis.fileName}</span>
              </div>
              <p class="text-sm text-muted-foreground">
                {formatSize(analysis.fileSize)} · ~{analysis.estimatedWords.toLocaleString()} words
              </p>
            </div>

            <!-- Content stats -->
            <div class="grid grid-cols-2 gap-3">
              <div class="p-3 bg-accent/30 rounded">
                <div class="text-2xl font-bold text-foreground">{analysis.paragraphCount}</div>
                <div class="text-sm text-muted-foreground">Paragraphs</div>
              </div>
              <div class="p-3 bg-accent/30 rounded">
                <div class="text-2xl font-bold text-foreground">{analysis.headingCount}</div>
                <div class="text-sm text-muted-foreground">Headings</div>
              </div>
              {#if analysis.imageCount > 0}
                <div class="p-3 bg-accent/30 rounded">
                  <div class="text-2xl font-bold text-foreground">{analysis.imageCount}</div>
                  <div class="text-sm text-muted-foreground">Images</div>
                </div>
              {/if}
              {#if analysis.listCount > 0}
                <div class="p-3 bg-accent/30 rounded">
                  <div class="text-2xl font-bold text-foreground">{analysis.listCount}</div>
                  <div class="text-sm text-muted-foreground">Lists</div>
                </div>
              {/if}
            </div>

            {#if analysis.tableCount > 0}
              <div class="p-3 bg-yellow-500/10 border border-yellow-500/20 rounded">
                <p class="text-sm text-yellow-600 dark:text-yellow-400">
                  This document contains {analysis.tableCount} table{analysis.tableCount > 1 ? 's' : ''}. Tables will be converted to plain text.
                </p>
              </div>
            {/if}
          </div>

        {:else if step === 'importing'}
          <!-- Importing Step -->
          <div class="space-y-4">
            <div class="text-center py-8">
              <svg class="animate-spin h-10 w-10 mx-auto text-primary mb-4" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
              </svg>
              <p class="font-medium text-foreground">Converting document...</p>
              <p class="text-sm text-muted-foreground mt-1">This may take a moment for large documents</p>
            </div>
          </div>

        {:else if step === 'complete' && result}
          <!-- Complete Step -->
          <div class="space-y-4">
            <div class="text-center py-4">
              <div class="w-16 h-16 mx-auto mb-4 bg-green-500/10 rounded-full flex items-center justify-center">
                <svg class="w-8 h-8 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <p class="font-medium text-foreground">Import Complete!</p>
              <p class="text-sm text-muted-foreground mt-1">Your document is ready</p>
            </div>

            <!-- Stats -->
            <div class="grid grid-cols-3 gap-3">
              <div class="p-3 bg-accent/30 rounded text-center">
                <div class="text-xl font-bold text-foreground">{result.stats.paragraphs}</div>
                <div class="text-xs text-muted-foreground">Paragraphs</div>
              </div>
              <div class="p-3 bg-accent/30 rounded text-center">
                <div class="text-xl font-bold text-foreground">{result.stats.headings}</div>
                <div class="text-xs text-muted-foreground">Headings</div>
              </div>
              <div class="p-3 bg-accent/30 rounded text-center">
                <div class="text-xl font-bold text-foreground">{result.stats.images}</div>
                <div class="text-xs text-muted-foreground">Images</div>
              </div>
            </div>

            {#if result.warnings.length > 0}
              <div class="p-3 bg-yellow-500/10 border border-yellow-500/20 rounded max-h-32 overflow-y-auto">
                <h3 class="text-sm font-medium text-yellow-600 dark:text-yellow-400 mb-1">
                  Warnings ({result.warnings.length})
                </h3>
                {#each result.warnings.slice(0, 5) as warn}
                  <p class="text-xs text-muted-foreground truncate">{warn.message}</p>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div class="px-6 py-4 border-t border-border flex justify-between">
        {#if step === 'select'}
          <button
            class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
            onclick={onClose}
          >
            Cancel
          </button>
          <div></div>
        {:else if step === 'analyze'}
          <button
            class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
            onclick={() => { step = 'select'; analysis = null; filePath = null; }}
          >
            Back
          </button>
          <button
            class="px-4 py-2 text-sm font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors"
            onclick={handleImport}
          >
            Import Document
          </button>
        {:else if step === 'importing'}
          <div></div>
          <button
            class="px-4 py-2 text-sm font-medium text-muted-foreground cursor-not-allowed"
            disabled
          >
            Importing...
          </button>
        {:else if step === 'complete'}
          <div></div>
          <button
            class="px-4 py-2 text-sm font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors"
            onclick={onClose}
          >
            Done
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
