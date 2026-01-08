<script lang="ts">
  import { recoveryStore, showRecoveryDialog, pendingRecoveries, type RecoveryFile } from '@midlight/stores';
  import { recoveryClient } from '$lib/recovery';

  interface Props {
    workspaceRoot: string;
    onRecoverFile?: (fileKey: string, content: string) => void;
  }

  let { workspaceRoot, onRecoverFile }: Props = $props();

  let dialogRef: HTMLDivElement | null = $state(null);
  let processing = $state<string | null>(null);

  // Format relative time from ISO string
  function formatRelativeTime(dateStr: string): string {
    const date = new Date(dateStr);
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);
    const days = Math.floor(diff / 86400000);

    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes} minute${minutes !== 1 ? 's' : ''} ago`;
    if (hours < 24) return `${hours} hour${hours !== 1 ? 's' : ''} ago`;
    if (days < 7) return `${days} day${days !== 1 ? 's' : ''} ago`;
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

  // Handle recovering a single file
  async function handleRecover(recovery: RecoveryFile) {
    processing = recovery.fileKey;
    try {
      // Call the onRecoverFile callback to apply the recovery
      if (onRecoverFile) {
        onRecoverFile(recovery.fileKey, recovery.walContent);
      }

      // Remove from pending list
      recoveryStore.removeRecovery(recovery.fileKey);
    } catch (error) {
      console.error('Failed to recover file:', error);
    } finally {
      processing = null;
    }
  }

  // Handle discarding a single file's recovery
  async function handleDiscard(fileKey: string) {
    processing = fileKey;
    try {
      await recoveryClient.discardRecovery(workspaceRoot, fileKey);
      recoveryStore.removeRecovery(fileKey);
    } catch (error) {
      console.error('Failed to discard recovery:', error);
    } finally {
      processing = null;
    }
  }

  // Handle recovering all files
  async function handleRecoverAll() {
    processing = 'all';
    try {
      const recoveries = $pendingRecoveries;
      for (const recovery of recoveries) {
        if (onRecoverFile) {
          onRecoverFile(recovery.fileKey, recovery.walContent);
        }
      }
      recoveryStore.clearPendingRecoveries();
    } catch (error) {
      console.error('Failed to recover all files:', error);
    } finally {
      processing = null;
    }
  }

  // Handle discarding all recoveries
  async function handleDiscardAll() {
    processing = 'all';
    try {
      await recoveryClient.discardAllRecovery(workspaceRoot);
      recoveryStore.clearPendingRecoveries();
    } catch (error) {
      console.error('Failed to discard all recoveries:', error);
    } finally {
      processing = null;
    }
  }

  // Close dialog
  function handleClose() {
    recoveryStore.closeDialog();
  }

  // Keyboard handling
  function handleKeyDown(e: KeyboardEvent) {
    if (!$showRecoveryDialog) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      handleClose();
    }
  }

  // Focus dialog when opened
  $effect(() => {
    if ($showRecoveryDialog && dialogRef) {
      dialogRef.focus();
    }
  });
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if $showRecoveryDialog && $pendingRecoveries.length > 0}
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
      aria-labelledby="recovery-dialog-title"
      tabindex="-1"
    >
      <!-- Header -->
      <div class="p-6 border-b border-border">
        <div class="flex items-center gap-3 mb-2">
          <div class="w-10 h-10 rounded-full bg-amber-500/10 flex items-center justify-center">
            <svg class="w-5 h-5 text-amber-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <div>
            <h2 id="recovery-dialog-title" class="text-lg font-semibold text-foreground">
              Recovered Unsaved Changes
            </h2>
            <p class="text-sm text-muted-foreground">
              {$pendingRecoveries.length} file{$pendingRecoveries.length !== 1 ? 's have' : ' has'} unsaved changes from a previous session
            </p>
          </div>
        </div>
      </div>

      <!-- File List -->
      <div class="flex-1 overflow-y-auto p-4">
        <div class="space-y-2">
          {#each $pendingRecoveries as recovery (recovery.fileKey)}
            <div class="bg-muted/50 rounded-lg p-4 border border-border">
              <div class="flex items-start justify-between gap-4">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <svg class="w-4 h-4 text-muted-foreground flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                    </svg>
                    <span class="font-medium text-foreground truncate">{getFileName(recovery.fileKey)}</span>
                  </div>
                  {#if getFolderPath(recovery.fileKey)}
                    <p class="text-xs text-muted-foreground mt-1 truncate pl-6">
                      {getFolderPath(recovery.fileKey)}
                    </p>
                  {/if}
                  <p class="text-xs text-muted-foreground mt-1 pl-6">
                    Last edited {formatRelativeTime(recovery.walTime)}
                  </p>
                </div>
                <div class="flex items-center gap-2 flex-shrink-0">
                  <button
                    class="px-3 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors disabled:opacity-50"
                    onclick={() => handleDiscard(recovery.fileKey)}
                    disabled={processing !== null}
                  >
                    Discard
                  </button>
                  <button
                    class="px-3 py-1.5 text-xs font-medium bg-primary hover:bg-primary/90 text-primary-foreground rounded transition-colors disabled:opacity-50"
                    onclick={() => handleRecover(recovery)}
                    disabled={processing !== null}
                  >
                    {processing === recovery.fileKey ? 'Recovering...' : 'Recover'}
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border flex items-center justify-between gap-4">
        <button
          class="px-4 py-2 text-sm font-medium text-destructive hover:bg-destructive/10 rounded transition-colors disabled:opacity-50"
          onclick={handleDiscardAll}
          disabled={processing !== null}
        >
          Discard All
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
            onclick={handleRecoverAll}
            disabled={processing !== null}
          >
            {processing === 'all' ? 'Recovering...' : 'Recover All'}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
