<script lang="ts">
  import { exportStore, exportProgress, exportError } from '@midlight/stores';

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  // Auto-close on success
  $effect(() => {
    if ($exportProgress && $exportProgress.current === $exportProgress.total && $exportProgress.total > 0) {
      // Delay to show completion
      const timer = setTimeout(() => {
        exportStore.completeExport();
        onClose();
      }, 1500);
      return () => clearTimeout(timer);
    }
  });

  // Get export type label
  function getExportLabel(): string {
    const state = $exportStore;
    switch (state.exportType) {
      case 'pdf':
        return 'PDF';
      case 'docx':
        return 'Word Document';
      default:
        return 'Document';
    }
  }

  function handleClose() {
    exportStore.clearError();
    onClose();
  }
</script>

{#if open}
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    role="presentation"
  >
    <div
      class="bg-card border border-border rounded-lg shadow-xl w-80 p-6"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="export-title"
    >
      <div class="flex items-center gap-3 mb-4">
        {#if $exportError}
          <div class="w-6 h-6 text-destructive">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </div>
          <h3 id="export-title" class="font-semibold text-foreground">Export Failed</h3>
        {:else if $exportProgress && $exportProgress.current === $exportProgress.total && $exportProgress.total > 0}
          <div class="w-6 h-6 text-green-500">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
            </svg>
          </div>
          <h3 id="export-title" class="font-semibold text-foreground">Export Complete</h3>
        {:else}
          <svg class="animate-spin h-6 w-6 text-primary" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
          </svg>
          <h3 id="export-title" class="font-semibold text-foreground">Exporting to {getExportLabel()}</h3>
        {/if}
      </div>

      {#if $exportError}
        <div class="space-y-4">
          <p class="text-sm text-destructive">{$exportError}</p>
          <button
            onclick={handleClose}
            class="w-full px-4 py-2 bg-primary text-primary-foreground rounded hover:bg-primary/90 transition-colors text-sm font-medium"
          >
            Close
          </button>
        </div>
      {:else}
        <div class="space-y-3">
          <div class="flex items-center gap-2 text-sm text-muted-foreground">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
            <span>{$exportProgress?.phase || 'Processing...'}</span>
          </div>

          {#if $exportProgress && $exportProgress.total > 0}
            {@const percentage = Math.round(($exportProgress.current / $exportProgress.total) * 100)}
            {#if $exportProgress.current < $exportProgress.total}
              <div class="w-full bg-accent rounded-full h-2 overflow-hidden">
                <div
                  class="bg-primary h-full transition-all duration-300 ease-out"
                  style="width: {percentage}%"
                ></div>
              </div>
              <div class="flex justify-between text-xs text-muted-foreground">
                <span>{$exportProgress.current} / {$exportProgress.total} elements</span>
                <span>{percentage}%</span>
              </div>
            {/if}
          {/if}

          {#if $exportProgress && $exportProgress.current === $exportProgress.total && $exportProgress.total > 0}
            <p class="text-sm text-green-600 dark:text-green-400">
              Your document has been exported successfully.
            </p>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}
