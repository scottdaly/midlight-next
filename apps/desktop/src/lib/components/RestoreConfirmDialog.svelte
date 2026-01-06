<script lang="ts">
  import type { Checkpoint } from '@midlight/core/types';

  interface Props {
    open: boolean;
    checkpoint: Checkpoint | null;
    onRestore: (createBackup: boolean) => void;
    onCancel: () => void;
  }

  let { open, checkpoint, onRestore, onCancel }: Props = $props();

  let createBackup = $state(true);
  let dialogRef: HTMLDivElement | null = $state(null);

  function handleRestore() {
    onRestore(createBackup);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!open) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    }
  }

  function formatDate(timestamp: string): string {
    return new Date(timestamp).toLocaleString();
  }

  // Focus the dialog when opened
  $effect(() => {
    if (open && dialogRef) {
      dialogRef.focus();
    }
  });
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open && checkpoint}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={onCancel}
    role="presentation"
  >
    <!-- Dialog -->
    <div
      bind:this={dialogRef}
      class="bg-card border border-border rounded-lg shadow-xl max-w-md w-full mx-4"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      tabindex="-1"
    >
      <!-- Header -->
      <div class="px-6 py-4 border-b border-border">
        <h2 id="dialog-title" class="text-lg font-semibold text-foreground">
          Restore Version
        </h2>
      </div>

      <!-- Content -->
      <div class="p-6 space-y-4">
        <!-- Warning -->
        <div class="flex gap-3 p-3 bg-amber-500/10 border border-amber-500/20 rounded-lg">
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-amber-500 flex-shrink-0 mt-0.5">
            <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
            <path d="M12 9v4"/>
            <path d="M12 17h.01"/>
          </svg>
          <div class="text-sm">
            <p class="font-medium text-amber-500">This will replace your current document</p>
            <p class="text-muted-foreground mt-1">
              Any unsaved changes will be lost.
            </p>
          </div>
        </div>

        <!-- Version info -->
        <div class="p-3 bg-muted rounded-lg">
          <div class="flex items-center gap-2 mb-2">
            {#if checkpoint.type === 'bookmark'}
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary">
                <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
              </svg>
            {:else}
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
                <circle cx="12" cy="12" r="10"/>
                <polyline points="12 6 12 12 16 14"/>
              </svg>
            {/if}
            <span class="font-medium text-foreground">
              {checkpoint.label || 'Auto-saved version'}
            </span>
          </div>
          <p class="text-sm text-muted-foreground">
            {formatDate(checkpoint.timestamp)}
          </p>
          {#if checkpoint.description}
            <p class="text-sm text-muted-foreground mt-1">
              {checkpoint.description}
            </p>
          {/if}
        </div>

        <!-- Backup option -->
        <label class="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            bind:checked={createBackup}
            class="mt-0.5 w-4 h-4 rounded border-input bg-background text-primary focus:ring-2 focus:ring-ring"
          />
          <div>
            <span class="text-sm font-medium text-foreground">Create backup first</span>
            <p class="text-xs text-muted-foreground mt-0.5">
              Save your current document as a version before restoring
            </p>
          </div>
        </label>
      </div>

      <!-- Buttons -->
      <div class="px-6 py-4 border-t border-border flex justify-end gap-3">
        <button
          class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          onclick={onCancel}
        >
          Cancel
        </button>
        <button
          class="px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
          onclick={handleRestore}
        >
          Restore Version
        </button>
      </div>
    </div>
  </div>
{/if}
