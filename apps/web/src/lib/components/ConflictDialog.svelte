<script lang="ts">
  import { sync, hasConflicts } from '@midlight/stores';
  import { syncClient, type ConflictDetails, type ConflictResolution } from '$lib/sync';
  import { fade, scale } from 'svelte/transition';

  let conflictDetails = $state<ConflictDetails | null>(null);
  let isLoading = $state(false);
  let isResolving = $state(false);
  let error = $state<string | null>(null);
  let selectedTab = $state<'local' | 'remote'>('local');

  // Load conflict details when active conflict changes
  $effect(() => {
    const conflict = $sync.activeConflict;
    if (conflict) {
      loadConflictDetails(conflict.id);
    } else {
      conflictDetails = null;
    }
  });

  async function loadConflictDetails(conflictId: string) {
    isLoading = true;
    error = null;
    try {
      conflictDetails = await syncClient.getConflictDetails(conflictId);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load conflict details';
    } finally {
      isLoading = false;
    }
  }

  async function resolveConflict(resolution: ConflictResolution) {
    if (!$sync.activeConflict) return;

    isResolving = true;
    error = null;
    try {
      await syncClient.resolveConflict($sync.activeConflict.id, resolution);
      sync.removeConflict($sync.activeConflict.id);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to resolve conflict';
    } finally {
      isResolving = false;
    }
  }

  function closeDialog() {
    sync.setActiveConflict(null);
  }

  function handleBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) {
      closeDialog();
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      closeDialog();
    }
  }

  function formatDate(dateString: string): string {
    return new Date(dateString).toLocaleString();
  }

  function truncateContent(content: string, maxLines = 20): string {
    const lines = content.split('\n');
    if (lines.length <= maxLines) return content;
    return lines.slice(0, maxLines).join('\n') + '\n...';
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $sync.activeConflict}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    onclick={handleBackdropClick}
    transition:fade={{ duration: 150 }}
    role="dialog"
    aria-modal="true"
    aria-labelledby="conflict-title"
  >
    <div
      class="bg-card border border-border rounded-lg shadow-xl max-w-4xl w-full mx-4 max-h-[90vh] overflow-hidden flex flex-col"
      transition:scale={{ duration: 150, start: 0.95 }}
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-border">
        <div>
          <h2 id="conflict-title" class="text-lg font-semibold text-foreground">
            Sync Conflict
          </h2>
          <p class="text-sm text-muted-foreground mt-1">
            {$sync.activeConflict.path}
          </p>
        </div>
        <button
          class="p-2 hover:bg-accent rounded-md transition-colors"
          onclick={closeDialog}
          aria-label="Close dialog"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-auto p-6">
        {#if isLoading}
          <div class="flex items-center justify-center py-12">
            <svg class="w-8 h-8 animate-spin text-primary" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
          </div>
        {:else if error}
          <div class="bg-destructive/10 text-destructive px-4 py-3 rounded-md">
            {error}
          </div>
        {:else if conflictDetails}
          <!-- Explanation -->
          <div class="bg-amber-500/10 border border-amber-500/20 rounded-md px-4 py-3 mb-6">
            <div class="flex items-start gap-3">
              <svg class="w-5 h-5 text-amber-500 mt-0.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              <div>
                <p class="text-sm text-foreground font-medium">
                  This document was edited on another device while offline
                </p>
                <p class="text-sm text-muted-foreground mt-1">
                  Choose which version to keep, or keep both as separate documents.
                </p>
              </div>
            </div>
          </div>

          <!-- Version comparison tabs -->
          <div class="flex border-b border-border mb-4">
            <button
              class="px-4 py-2 text-sm font-medium transition-colors"
              class:text-primary={selectedTab === 'local'}
              class:border-b-2={selectedTab === 'local'}
              class:border-primary={selectedTab === 'local'}
              class:text-muted-foreground={selectedTab !== 'local'}
              onclick={() => selectedTab = 'local'}
            >
              Your Version
              <span class="ml-2 text-xs text-muted-foreground">
                (v{conflictDetails.local?.version ?? '?'})
              </span>
            </button>
            <button
              class="px-4 py-2 text-sm font-medium transition-colors"
              class:text-primary={selectedTab === 'remote'}
              class:border-b-2={selectedTab === 'remote'}
              class:border-primary={selectedTab === 'remote'}
              class:text-muted-foreground={selectedTab !== 'remote'}
              onclick={() => selectedTab = 'remote'}
            >
              Other Version
              <span class="ml-2 text-xs text-muted-foreground">
                (v{conflictDetails.remote?.version ?? '?'})
              </span>
            </button>
          </div>

          <!-- Content preview -->
          <div class="bg-muted/30 rounded-md border border-border overflow-hidden">
            <pre class="p-4 text-sm font-mono text-foreground overflow-auto max-h-64">{#if selectedTab === 'local'}{conflictDetails.local?.content ? truncateContent(conflictDetails.local.content) : 'Content not available'}{:else}{conflictDetails.remote?.content ? truncateContent(conflictDetails.remote.content) : 'Content not available'}{/if}</pre>
          </div>

          <!-- Metadata -->
          <div class="mt-4 grid grid-cols-2 gap-4 text-sm">
            <div class="bg-muted/20 rounded-md p-3">
              <p class="font-medium text-foreground">Your Version</p>
              <p class="text-muted-foreground mt-1">
                Version {conflictDetails.local?.version ?? 'Unknown'}
              </p>
            </div>
            <div class="bg-muted/20 rounded-md p-3">
              <p class="font-medium text-foreground">Other Version</p>
              <p class="text-muted-foreground mt-1">
                Version {conflictDetails.remote?.version ?? 'Unknown'}
              </p>
            </div>
          </div>
        {/if}
      </div>

      <!-- Actions -->
      <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-border bg-muted/20">
        <button
          class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
          onclick={closeDialog}
          disabled={isResolving}
        >
          Decide Later
        </button>

        <button
          class="px-4 py-2 text-sm font-medium bg-secondary text-secondary-foreground hover:bg-secondary/80 rounded-md transition-colors disabled:opacity-50"
          onclick={() => resolveConflict('both')}
          disabled={isResolving || isLoading}
        >
          Keep Both
        </button>

        <button
          class="px-4 py-2 text-sm font-medium bg-secondary text-secondary-foreground hover:bg-secondary/80 rounded-md transition-colors disabled:opacity-50"
          onclick={() => resolveConflict('remote')}
          disabled={isResolving || isLoading}
        >
          Keep Theirs
        </button>

        <button
          class="px-4 py-2 text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 rounded-md transition-colors disabled:opacity-50"
          onclick={() => resolveConflict('local')}
          disabled={isResolving || isLoading}
        >
          {#if isResolving}
            <svg class="w-4 h-4 animate-spin inline mr-2" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
          {/if}
          Keep Mine
        </button>
      </div>
    </div>
  </div>
{/if}
