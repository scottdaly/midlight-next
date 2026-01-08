<script lang="ts">
  import {
    fileWatcherStore,
    showExternalChangeDialog,
    pendingExternalChanges,
    changesByType,
    type ExternalChange,
  } from '@midlight/stores';

  interface Props {
    onReloadFile?: (fileKey: string) => Promise<void>;
    onCloseFile?: (fileKey: string) => void;
    onRefreshFileTree?: () => void;
  }

  let { onReloadFile, onCloseFile, onRefreshFileTree }: Props = $props();

  let dialogRef: HTMLDivElement | null = $state(null);
  let processing = $state<string | null>(null);

  // Format relative time from Date
  function formatRelativeTime(date: Date): string {
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);

    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes} minute${minutes !== 1 ? 's' : ''} ago`;
    if (hours < 24) return `${hours} hour${hours !== 1 ? 's' : ''} ago`;
    return date.toLocaleDateString();
  }

  // Get file name from file key
  function getFileName(fileKey: string): string {
    const parts = fileKey.split('/');
    return parts[parts.length - 1];
  }

  // Get folder path from file key
  function getFolderPath(fileKey: string): string {
    const parts = fileKey.split('/');
    if (parts.length <= 1) return '';
    return parts.slice(0, -1).join('/');
  }

  // Get change type label
  function getChangeTypeLabel(changeType: ExternalChange['changeType']): string {
    switch (changeType) {
      case 'modify':
        return 'Modified externally';
      case 'create':
        return 'Created externally';
      case 'delete':
        return 'Deleted externally';
      default:
        return 'Changed';
    }
  }

  // Get change type color class
  function getChangeTypeColor(changeType: ExternalChange['changeType']): string {
    switch (changeType) {
      case 'modify':
        return 'text-blue-500';
      case 'create':
        return 'text-green-500';
      case 'delete':
        return 'text-red-500';
      default:
        return 'text-muted-foreground';
    }
  }

  // Handle reloading a modified file from disk
  async function handleReload(change: ExternalChange) {
    processing = change.fileKey;
    try {
      if (onReloadFile) {
        await onReloadFile(change.fileKey);
      }
      fileWatcherStore.removeChange(change.fileKey);
    } catch (error) {
      console.error('Failed to reload file:', error);
    } finally {
      processing = null;
    }
  }

  // Handle keeping the current version (ignore external change)
  function handleKeep(change: ExternalChange) {
    fileWatcherStore.removeChange(change.fileKey);
  }

  // Handle a deleted file - close it
  function handleCloseDeleted(change: ExternalChange) {
    if (onCloseFile) {
      onCloseFile(change.fileKey);
    }
    fileWatcherStore.removeChange(change.fileKey);
  }

  // Handle a created file - refresh tree
  function handleRefreshForCreated(change: ExternalChange) {
    if (onRefreshFileTree) {
      onRefreshFileTree();
    }
    fileWatcherStore.removeChange(change.fileKey);
  }

  // Handle reload all modified files
  async function handleReloadAll() {
    processing = 'all';
    try {
      const changes = $pendingExternalChanges;
      for (const change of changes) {
        if (change.changeType === 'modify' && onReloadFile) {
          await onReloadFile(change.fileKey);
        } else if (change.changeType === 'delete' && onCloseFile) {
          onCloseFile(change.fileKey);
        } else if (change.changeType === 'create' && onRefreshFileTree) {
          onRefreshFileTree();
        }
      }
      fileWatcherStore.clearAllChanges();
    } catch (error) {
      console.error('Failed to handle all changes:', error);
    } finally {
      processing = null;
    }
  }

  // Handle ignoring all changes
  function handleIgnoreAll() {
    fileWatcherStore.clearAllChanges();
  }

  // Close dialog
  function handleClose() {
    fileWatcherStore.closeDialog();
  }

  // Keyboard handling
  function handleKeyDown(e: KeyboardEvent) {
    if (!$showExternalChangeDialog) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      handleClose();
    }
  }

  // Focus dialog when opened
  $effect(() => {
    if ($showExternalChangeDialog && dialogRef) {
      dialogRef.focus();
    }
  });
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if $showExternalChangeDialog && $pendingExternalChanges.length > 0}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={handleClose}
    role="presentation"
  >
    <!-- Dialog -->
    <div
      bind:this={dialogRef}
      class="bg-card border border-border rounded-lg shadow-xl max-w-lg w-full mx-4 max-h-[80vh] flex flex-col"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="external-change-dialog-title"
      tabindex="-1"
    >
      <!-- Header -->
      <div class="p-6 border-b border-border">
        <div class="flex items-center gap-3 mb-2">
          <div class="w-10 h-10 rounded-full bg-blue-500/10 flex items-center justify-center">
            <svg class="w-5 h-5 text-blue-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </div>
          <div>
            <h2 id="external-change-dialog-title" class="text-lg font-semibold text-foreground">
              Files Changed Externally
            </h2>
            <p class="text-sm text-muted-foreground">
              {$pendingExternalChanges.length} file{$pendingExternalChanges.length !== 1 ? 's were' : ' was'} changed outside Midlight
            </p>
          </div>
        </div>
      </div>

      <!-- File List -->
      <div class="flex-1 overflow-y-auto p-4">
        <div class="space-y-2">
          {#each $pendingExternalChanges as change (change.fileKey)}
            <div class="bg-muted/50 rounded-lg p-4 border border-border">
              <div class="flex items-start justify-between gap-4">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    {#if change.changeType === 'modify'}
                      <svg class="w-4 h-4 text-blue-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                      </svg>
                    {:else if change.changeType === 'create'}
                      <svg class="w-4 h-4 text-green-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6v6m0 0v6m0-6h6m-6 0H6" />
                      </svg>
                    {:else if change.changeType === 'delete'}
                      <svg class="w-4 h-4 text-red-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    {/if}
                    <span class="font-medium text-foreground truncate">{getFileName(change.fileKey)}</span>
                  </div>
                  {#if getFolderPath(change.fileKey)}
                    <p class="text-xs text-muted-foreground mt-1 truncate pl-6">
                      {getFolderPath(change.fileKey)}
                    </p>
                  {/if}
                  <p class="text-xs {getChangeTypeColor(change.changeType)} mt-1 pl-6">
                    {getChangeTypeLabel(change.changeType)} {formatRelativeTime(change.timestamp)}
                  </p>
                </div>
                <div class="flex items-center gap-2 flex-shrink-0">
                  {#if change.changeType === 'modify'}
                    <button
                      class="px-3 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors disabled:opacity-50"
                      onclick={() => handleKeep(change)}
                      disabled={processing !== null}
                    >
                      Keep Mine
                    </button>
                    <button
                      class="px-3 py-1.5 text-xs font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors disabled:opacity-50"
                      onclick={() => handleReload(change)}
                      disabled={processing !== null}
                    >
                      {processing === change.fileKey ? 'Reloading...' : 'Reload'}
                    </button>
                  {:else if change.changeType === 'delete'}
                    <button
                      class="px-3 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors disabled:opacity-50"
                      onclick={() => handleKeep(change)}
                      disabled={processing !== null}
                    >
                      Ignore
                    </button>
                    <button
                      class="px-3 py-1.5 text-xs font-medium bg-destructive hover:bg-destructive/90 text-destructive-foreground rounded transition-colors disabled:opacity-50"
                      onclick={() => handleCloseDeleted(change)}
                      disabled={processing !== null}
                    >
                      Close File
                    </button>
                  {:else if change.changeType === 'create'}
                    <button
                      class="px-3 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors disabled:opacity-50"
                      onclick={() => handleKeep(change)}
                      disabled={processing !== null}
                    >
                      Ignore
                    </button>
                    <button
                      class="px-3 py-1.5 text-xs font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors disabled:opacity-50"
                      onclick={() => handleRefreshForCreated(change)}
                      disabled={processing !== null}
                    >
                      Show in Tree
                    </button>
                  {/if}
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border flex items-center justify-between gap-4">
        <button
          class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors disabled:opacity-50"
          onclick={handleIgnoreAll}
          disabled={processing !== null}
        >
          Ignore All
        </button>
        <div class="flex items-center gap-3">
          <button
            class="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors disabled:opacity-50"
            onclick={handleClose}
            disabled={processing !== null}
          >
            Later
          </button>
          <button
            class="px-4 py-2 text-sm font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors disabled:opacity-50"
            onclick={handleReloadAll}
            disabled={processing !== null}
          >
            {processing === 'all' ? 'Processing...' : 'Accept All Changes'}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
