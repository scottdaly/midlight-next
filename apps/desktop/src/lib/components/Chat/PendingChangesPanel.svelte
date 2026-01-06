<script lang="ts">
  /**
   * PendingChangesPanel - Shows pending AI changes for review
   * Allows users to accept or reject individual changes
   */

  import { agent, pendingChanges, hasPendingChanges, fileSystem } from '@midlight/stores';
  import type { PendingChange } from '@midlight/stores';
  import { DiffDisplay } from '@midlight/ui';
  import { invoke } from '@tauri-apps/api/core';

  interface Props {
    onClose?: () => void;
  }

  let { onClose }: Props = $props();

  // Track which change is expanded to show diff
  let expandedChangeId = $state<string | null>(null);

  function toggleExpand(changeId: string) {
    if (expandedChangeId === changeId) {
      expandedChangeId = null;
    } else {
      expandedChangeId = changeId;
    }
  }

  async function acceptChange(change: PendingChange) {
    try {
      // The change is already applied to the file, just remove from pending
      agent.acceptChange(change.changeId);
      // Refresh file tree to show updated content
      await fileSystem.refresh();
    } catch (error) {
      console.error('Failed to accept change:', error);
    }
  }

  async function rejectChange(change: PendingChange) {
    try {
      // Restore the original content
      await invoke('write_file', {
        path: change.path,
        content: change.originalContent,
      });
      // Remove from pending
      agent.rejectChange(change.changeId);
      // Refresh file tree
      await fileSystem.refresh();
    } catch (error) {
      console.error('Failed to reject change:', error);
    }
  }

  async function acceptAll() {
    const changes = [...$pendingChanges];
    for (const change of changes) {
      agent.acceptChange(change.changeId);
    }
    await fileSystem.refresh();
  }

  async function rejectAll() {
    const changes = [...$pendingChanges];
    for (const change of changes) {
      try {
        await invoke('write_file', {
          path: change.path,
          content: change.originalContent,
        });
        agent.rejectChange(change.changeId);
      } catch (error) {
        console.error('Failed to reject change:', error);
      }
    }
    await fileSystem.refresh();
  }

  // Get filename from path
  function getFileName(path: string): string {
    const parts = path.split('/');
    const name = parts[parts.length - 1];
    if (name.endsWith('.midlight')) {
      return name.slice(0, -9);
    }
    return name;
  }

  // Format relative time
  function formatTime(isoString: string): string {
    const date = new Date(isoString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;

    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;

    return date.toLocaleDateString();
  }
</script>

<div class="h-full flex flex-col">
  <!-- Header -->
  <div class="h-10 border-b border-border flex items-center justify-between px-3">
    <div class="flex items-center gap-2">
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-yellow-500">
        <path d="M12 9v4"/>
        <path d="M12 17h.01"/>
        <path d="M3.586 14.414A2 2 0 0 0 3 15.828V21a1 1 0 0 0 1 1h16a1 1 0 0 0 1-1v-5.172a2 2 0 0 0-.586-1.414l-6.293-6.293a2 2 0 0 0-1.414-.586h-1.414a2 2 0 0 0-1.414.586z"/>
      </svg>
      <span class="text-sm font-medium">Pending Changes</span>
      {#if $hasPendingChanges}
        <span class="px-1.5 py-0.5 text-xs rounded-full bg-yellow-500/20 text-yellow-400">
          {$pendingChanges.length}
        </span>
      {/if}
    </div>
    {#if onClose}
      <button
        onclick={onClose}
        class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground"
        title="Close"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18"/>
          <line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
      </button>
    {/if}
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-auto">
    {#if !$hasPendingChanges}
      <div class="flex flex-col items-center justify-center h-full text-center px-4">
        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground/50 mb-3">
          <path d="M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10z"/>
          <path d="m9 12 2 2 4-4"/>
        </svg>
        <p class="text-sm text-muted-foreground">No pending changes</p>
        <p class="text-xs text-muted-foreground/70 mt-1">
          AI edits will appear here for review
        </p>
      </div>
    {:else}
      <!-- Bulk Actions -->
      <div class="flex gap-2 p-3 border-b border-border">
        <button
          onclick={acceptAll}
          class="flex-1 px-3 py-1.5 text-xs rounded-md bg-green-600 hover:bg-green-500 text-white transition-colors flex items-center justify-center gap-1"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="20 6 9 17 4 12"/>
          </svg>
          Accept All
        </button>
        <button
          onclick={rejectAll}
          class="flex-1 px-3 py-1.5 text-xs rounded-md bg-neutral-600 hover:bg-neutral-500 text-white transition-colors flex items-center justify-center gap-1"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
          Reject All
        </button>
      </div>

      <!-- Changes List -->
      <div class="divide-y divide-border">
        {#each $pendingChanges as change (change.changeId)}
          <div class="p-3">
            <!-- Change Header -->
            <button
              onclick={() => toggleExpand(change.changeId)}
              class="w-full text-left flex items-start gap-2 hover:bg-accent/30 -m-1 p-1 rounded transition-colors"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                class="mt-0.5 text-muted-foreground transition-transform {expandedChangeId === change.changeId ? 'rotate-90' : ''}"
              >
                <polyline points="9 18 15 12 9 6"/>
              </svg>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium truncate">
                    {getFileName(change.path)}
                  </span>
                  <span class="text-xs text-muted-foreground">
                    {formatTime(change.createdAt)}
                  </span>
                </div>
                {#if change.description}
                  <p class="text-xs text-muted-foreground mt-0.5 line-clamp-2">
                    {change.description}
                  </p>
                {/if}
              </div>
            </button>

            <!-- Expanded Diff -->
            {#if expandedChangeId === change.changeId}
              <div class="mt-3">
                <DiffDisplay
                  originalContent={change.originalContent}
                  newContent={change.newContent}
                  fileName={getFileName(change.path)}
                />

                <!-- Individual Actions -->
                <div class="flex gap-2 mt-3">
                  <button
                    onclick={() => acceptChange(change)}
                    class="flex-1 px-3 py-1.5 text-xs rounded-md bg-green-600 hover:bg-green-500 text-white transition-colors"
                  >
                    Accept
                  </button>
                  <button
                    onclick={() => rejectChange(change)}
                    class="flex-1 px-3 py-1.5 text-xs rounded-md bg-neutral-600 hover:bg-neutral-500 text-white transition-colors"
                  >
                    Reject
                  </button>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
