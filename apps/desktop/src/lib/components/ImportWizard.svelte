<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { importStore } from '@midlight/stores';
  import {
    importClient,
    defaultImportOptions,
    defaultNotionOptions,
    type ImportAnalysis,
    type ImportOptions,
    type NotionImportOptions,
    type ImportProgress,
    type ImportResult,
    type ImportSourceType,
  } from '$lib/import';

  interface Props {
    open: boolean;
    onClose: () => void;
    initialPath?: string;
    onComplete?: (result: ImportResult) => void;
  }

  let { open, onClose, initialPath, onComplete }: Props = $props();

  type ImportStep = 'select' | 'analyze' | 'options' | 'importing' | 'complete';

  // State
  let step: ImportStep = $state('select');
  let sourcePath: string | null = $state(null);
  let sourceType: ImportSourceType | null = $state(null);
  let analysis: ImportAnalysis | null = $state(null);
  let options: ImportOptions = $state({ ...defaultImportOptions });
  let notionOptions: NotionImportOptions = $state({ ...defaultNotionOptions });
  let progress: ImportProgress | null = $state(null);
  let result: ImportResult | null = $state(null);
  let error: string | null = $state(null);
  let isAnalyzing: boolean = $state(false);
  let unlistenProgress: (() => void) | null = $state(null);

  // Get destination path - use parent of source for now
  function getDestPath(): string {
    if (!sourcePath) return '';
    const parts = sourcePath.split('/');
    parts.pop();
    return parts.join('/') + '/imported';
  }

  // Handle initial path if provided
  $effect(() => {
    if (open && initialPath && step === 'select') {
      sourcePath = initialPath;
      handleAnalyze();
    }
  });

  // Reset state when dialog closes
  $effect(() => {
    if (!open) {
      step = 'select';
      sourcePath = null;
      sourceType = null;
      analysis = null;
      options = { ...defaultImportOptions };
      notionOptions = { ...defaultNotionOptions };
      progress = null;
      result = null;
      error = null;
      isAnalyzing = false;
    }
  });

  // Cleanup progress listener
  onDestroy(() => {
    if (unlistenProgress) {
      unlistenProgress();
    }
  });

  // Select folder
  async function handleSelectFolder() {
    try {
      const selected = await importClient.selectFolder();
      if (selected) {
        sourcePath = selected;
        await handleAnalyze();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  // Analyze the source
  async function handleAnalyze() {
    if (!sourcePath) return;

    isAnalyzing = true;
    error = null;

    try {
      // Detect source type
      sourceType = await importClient.detectSourceType(sourcePath);

      // Analyze based on type
      if (sourceType === 'obsidian') {
        analysis = await importClient.analyzeObsidian(sourcePath);
      } else if (sourceType === 'notion') {
        analysis = await importClient.analyzeNotion(sourcePath);
      } else {
        // For generic, use obsidian analysis (same structure)
        analysis = await importClient.analyzeObsidian(sourcePath);
      }

      step = 'analyze';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      isAnalyzing = false;
    }
  }

  // Start import
  async function handleImport() {
    if (!analysis || !sourceType) return;

    step = 'importing';
    error = null;

    try {
      // Setup progress listener
      const unlisten = await importClient.onProgress((p) => {
        progress = p;
        importStore.updateProgress(p);
      });
      unlistenProgress = unlisten;

      // Start import tracking
      importStore.startImport(analysis.sourcePath, sourceType);

      // Run import
      const destPath = getDestPath();
      if (sourceType === 'notion') {
        result = await importClient.importNotion(analysis, destPath, notionOptions);
      } else {
        result = await importClient.importObsidian(analysis, destPath, options);
      }

      // Complete
      importStore.completeImport(result);
      step = 'complete';

      if (onComplete) {
        onComplete(result);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      importStore.cancelImport();
      step = 'analyze'; // Go back to analysis
    } finally {
      if (unlistenProgress) {
        unlistenProgress();
        unlistenProgress = null;
      }
    }
  }

  // Cancel import
  async function handleCancel() {
    try {
      await importClient.cancel();
      importStore.cancelImport();
      step = 'analyze';
    } catch (e) {
      // Ignore cancel errors
    }
  }

  // Format file size
  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  // Get source type label
  function getSourceLabel(type: ImportSourceType): string {
    switch (type) {
      case 'obsidian':
        return 'Obsidian Vault';
      case 'notion':
        return 'Notion Export';
      default:
        return 'Folder';
    }
  }

  // Get phase label
  function getPhaseLabel(phase: string): string {
    switch (phase) {
      case 'analyzing':
        return 'Analyzing...';
      case 'converting':
        return 'Converting files...';
      case 'copying':
        return 'Copying attachments...';
      case 'finalizing':
        return 'Finalizing...';
      case 'complete':
        return 'Complete!';
      default:
        return phase;
    }
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
      aria-labelledby="import-title"
    >
      <!-- Header -->
      <div class="px-6 py-4 border-b border-border">
        <h2 id="import-title" class="text-lg font-semibold text-foreground">
          {#if step === 'select'}
            Import Files
          {:else if step === 'analyze'}
            {#if sourceType}
              Import {getSourceLabel(sourceType)}
            {:else}
              Analyzing...
            {/if}
          {:else if step === 'options'}
            Import Options
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
              Import your documents from an Obsidian vault, Notion export, or any folder containing Markdown files.
            </p>

            <button
              class="w-full p-8 border-2 border-dashed border-border rounded-lg hover:border-primary/50 hover:bg-accent/50 transition-colors text-center"
              onclick={handleSelectFolder}
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
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                  <p class="font-medium text-foreground">Choose a folder to import</p>
                  <p class="text-sm mt-1">Click to browse</p>
                </div>
              {/if}
            </button>
          </div>

        {:else if step === 'analyze' && analysis}
          <!-- Analysis Step -->
          <div class="space-y-4">
            <div class="p-3 bg-accent/50 rounded-lg">
              <div class="flex items-center gap-2 mb-2">
                <span class="font-medium text-foreground">
                  {getSourceLabel(analysis.sourceType)}
                </span>
                <span class="text-xs px-2 py-0.5 bg-primary/10 text-primary rounded">
                  {analysis.sourceType}
                </span>
              </div>
              <p class="text-sm text-muted-foreground truncate" title={analysis.sourcePath}>
                {analysis.sourcePath}
              </p>
            </div>

            <!-- File counts -->
            <div class="grid grid-cols-2 gap-3">
              <div class="p-3 bg-accent/30 rounded">
                <div class="text-2xl font-bold text-foreground">{analysis.markdownFiles}</div>
                <div class="text-sm text-muted-foreground">Markdown files</div>
              </div>
              <div class="p-3 bg-accent/30 rounded">
                <div class="text-2xl font-bold text-foreground">{analysis.attachments}</div>
                <div class="text-sm text-muted-foreground">Attachments</div>
              </div>
            </div>

            <!-- Features detected -->
            {#if analysis.sourceType === 'obsidian'}
              <div class="space-y-2">
                <h3 class="text-sm font-medium text-foreground">Features Detected</h3>
                <div class="flex flex-wrap gap-2">
                  {#if analysis.wikiLinks > 0}
                    <span class="text-xs px-2 py-1 bg-blue-500/10 text-blue-600 dark:text-blue-400 rounded">
                      {analysis.wikiLinks} wiki links
                    </span>
                  {/if}
                  {#if analysis.callouts > 0}
                    <span class="text-xs px-2 py-1 bg-green-500/10 text-green-600 dark:text-green-400 rounded">
                      {analysis.callouts} callouts
                    </span>
                  {/if}
                  {#if analysis.frontMatter > 0}
                    <span class="text-xs px-2 py-1 bg-purple-500/10 text-purple-600 dark:text-purple-400 rounded">
                      {analysis.frontMatter} with front matter
                    </span>
                  {/if}
                  {#if analysis.dataviewBlocks > 0}
                    <span class="text-xs px-2 py-1 bg-orange-500/10 text-orange-600 dark:text-orange-400 rounded">
                      {analysis.dataviewBlocks} dataview blocks
                    </span>
                  {/if}
                </div>
              </div>
            {:else if analysis.sourceType === 'notion'}
              <div class="space-y-2">
                <h3 class="text-sm font-medium text-foreground">Features Detected</h3>
                <div class="flex flex-wrap gap-2">
                  {#if analysis.csvDatabases > 0}
                    <span class="text-xs px-2 py-1 bg-blue-500/10 text-blue-600 dark:text-blue-400 rounded">
                      {analysis.csvDatabases} CSV databases
                    </span>
                  {/if}
                  {#if analysis.folders > 0}
                    <span class="text-xs px-2 py-1 bg-green-500/10 text-green-600 dark:text-green-400 rounded">
                      {analysis.folders} folders
                    </span>
                  {/if}
                </div>
              </div>
            {/if}

            <!-- Warnings -->
            {#if analysis.accessWarnings.length > 0}
              <div class="p-3 bg-yellow-500/10 border border-yellow-500/20 rounded">
                <h3 class="text-sm font-medium text-yellow-600 dark:text-yellow-400 mb-1">
                  Access Warnings ({analysis.accessWarnings.length})
                </h3>
                <p class="text-xs text-muted-foreground">
                  Some files could not be read and will be skipped.
                </p>
              </div>
            {/if}
          </div>

        {:else if step === 'options'}
          <!-- Options Step -->
          <div class="space-y-4">
            <div class="space-y-3">
              {#if sourceType === 'obsidian'}
                <label class="flex items-center gap-3 p-3 bg-accent/30 rounded hover:bg-accent/50 cursor-pointer">
                  <input type="checkbox" bind:checked={options.convertWikiLinks} class="rounded" />
                  <div>
                    <div class="font-medium text-sm">Convert wiki links</div>
                    <div class="text-xs text-muted-foreground">Convert [[links]] to standard [links]()</div>
                  </div>
                </label>
                <label class="flex items-center gap-3 p-3 bg-accent/30 rounded hover:bg-accent/50 cursor-pointer">
                  <input type="checkbox" bind:checked={options.convertCallouts} class="rounded" />
                  <div>
                    <div class="font-medium text-sm">Convert callouts</div>
                    <div class="text-xs text-muted-foreground">Convert Obsidian callout syntax to blockquotes</div>
                  </div>
                </label>
              {:else if sourceType === 'notion'}
                <label class="flex items-center gap-3 p-3 bg-accent/30 rounded hover:bg-accent/50 cursor-pointer">
                  <input type="checkbox" bind:checked={notionOptions.removeUuids} class="rounded" />
                  <div>
                    <div class="font-medium text-sm">Remove UUIDs from filenames</div>
                    <div class="text-xs text-muted-foreground">Strip Notion's UUID suffixes from file names</div>
                  </div>
                </label>
                <label class="flex items-center gap-3 p-3 bg-accent/30 rounded hover:bg-accent/50 cursor-pointer">
                  <input type="checkbox" bind:checked={notionOptions.convertCsvToTables} class="rounded" />
                  <div>
                    <div class="font-medium text-sm">Convert CSV to tables</div>
                    <div class="text-xs text-muted-foreground">Convert CSV databases to Markdown tables</div>
                  </div>
                </label>
              {/if}

              <!-- Common options -->
              <label class="flex items-center gap-3 p-3 bg-accent/30 rounded hover:bg-accent/50 cursor-pointer">
                <input type="checkbox" bind:checked={options.copyAttachments} class="rounded" />
                <div>
                  <div class="font-medium text-sm">Copy attachments</div>
                  <div class="text-xs text-muted-foreground">Include images and other media files</div>
                </div>
              </label>
              <label class="flex items-center gap-3 p-3 bg-accent/30 rounded hover:bg-accent/50 cursor-pointer">
                <input type="checkbox" bind:checked={options.preserveFolderStructure} class="rounded" />
                <div>
                  <div class="font-medium text-sm">Preserve folder structure</div>
                  <div class="text-xs text-muted-foreground">Maintain the original folder hierarchy</div>
                </div>
              </label>
              <label class="flex items-center gap-3 p-3 bg-accent/30 rounded hover:bg-accent/50 cursor-pointer">
                <input type="checkbox" bind:checked={options.skipEmptyPages} class="rounded" />
                <div>
                  <div class="font-medium text-sm">Skip empty pages</div>
                  <div class="text-xs text-muted-foreground">Don't import blank files</div>
                </div>
              </label>
            </div>
          </div>

        {:else if step === 'importing'}
          <!-- Importing Step -->
          <div class="space-y-4">
            <div class="text-center py-4">
              <svg class="animate-spin h-10 w-10 mx-auto text-primary mb-4" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
              </svg>
              {#if progress}
                <p class="font-medium text-foreground">{getPhaseLabel(progress.phase)}</p>
                <p class="text-sm text-muted-foreground mt-1 truncate" title={progress.currentFile}>
                  {progress.currentFile || 'Processing...'}
                </p>
              {:else}
                <p class="font-medium text-foreground">Starting import...</p>
              {/if}
            </div>

            {#if progress && progress.total > 0}
              <div class="space-y-2">
                <div class="h-2 bg-accent rounded-full overflow-hidden">
                  <div
                    class="h-full bg-primary transition-all duration-300"
                    style="width: {Math.round((progress.current / progress.total) * 100)}%"
                  ></div>
                </div>
                <div class="flex justify-between text-xs text-muted-foreground">
                  <span>{progress.current} / {progress.total} files</span>
                  <span>{Math.round((progress.current / progress.total) * 100)}%</span>
                </div>
              </div>
            {/if}

            {#if progress && progress.errors.length > 0}
              <div class="p-3 bg-destructive/10 border border-destructive/20 rounded max-h-24 overflow-y-auto">
                <h3 class="text-sm font-medium text-destructive mb-1">Errors ({progress.errors.length})</h3>
                {#each progress.errors.slice(0, 5) as err}
                  <p class="text-xs text-muted-foreground truncate">{err.file}: {err.message}</p>
                {/each}
              </div>
            {/if}
          </div>

        {:else if step === 'complete' && result}
          <!-- Complete Step -->
          <div class="space-y-4">
            <div class="text-center py-4">
              {#if result.success}
                <div class="w-16 h-16 mx-auto mb-4 bg-green-500/10 rounded-full flex items-center justify-center">
                  <svg class="w-8 h-8 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
                <p class="font-medium text-foreground">Import Complete!</p>
              {:else}
                <div class="w-16 h-16 mx-auto mb-4 bg-yellow-500/10 rounded-full flex items-center justify-center">
                  <svg class="w-8 h-8 text-yellow-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                </div>
                <p class="font-medium text-foreground">Import completed with warnings</p>
              {/if}
            </div>

            <!-- Stats -->
            <div class="grid grid-cols-3 gap-3">
              <div class="p-3 bg-accent/30 rounded text-center">
                <div class="text-xl font-bold text-foreground">{result.filesImported}</div>
                <div class="text-xs text-muted-foreground">Files imported</div>
              </div>
              <div class="p-3 bg-accent/30 rounded text-center">
                <div class="text-xl font-bold text-foreground">{result.linksConverted}</div>
                <div class="text-xs text-muted-foreground">Links converted</div>
              </div>
              <div class="p-3 bg-accent/30 rounded text-center">
                <div class="text-xl font-bold text-foreground">{result.attachmentsCopied}</div>
                <div class="text-xs text-muted-foreground">Attachments</div>
              </div>
            </div>

            {#if result.errors.length > 0}
              <div class="p-3 bg-destructive/10 border border-destructive/20 rounded max-h-32 overflow-y-auto">
                <h3 class="text-sm font-medium text-destructive mb-1">Errors ({result.errors.length})</h3>
                {#each result.errors as err}
                  <p class="text-xs text-muted-foreground truncate">{err.file}: {err.message}</p>
                {/each}
              </div>
            {/if}

            {#if result.warnings.length > 0}
              <div class="p-3 bg-yellow-500/10 border border-yellow-500/20 rounded max-h-32 overflow-y-auto">
                <h3 class="text-sm font-medium text-yellow-600 dark:text-yellow-400 mb-1">
                  Warnings ({result.warnings.length})
                </h3>
                {#each result.warnings.slice(0, 10) as warn}
                  <p class="text-xs text-muted-foreground truncate">{warn.file}: {warn.message}</p>
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
            onclick={() => { step = 'select'; analysis = null; }}
          >
            Back
          </button>
          <div class="flex gap-2">
            <button
              class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
              onclick={() => { step = 'options'; }}
            >
              Customize
            </button>
            <button
              class="px-4 py-2 text-sm font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors"
              onclick={handleImport}
            >
              Import
            </button>
          </div>
        {:else if step === 'options'}
          <button
            class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
            onclick={() => { step = 'analyze'; }}
          >
            Back
          </button>
          <button
            class="px-4 py-2 text-sm font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors"
            onclick={handleImport}
          >
            Import
          </button>
        {:else if step === 'importing'}
          <div></div>
          <button
            class="px-4 py-2 text-sm font-medium text-destructive hover:bg-destructive/10 rounded transition-colors"
            onclick={handleCancel}
          >
            Cancel Import
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
