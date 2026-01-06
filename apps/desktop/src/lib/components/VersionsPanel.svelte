<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { fileSystem, versions, activeFile } from '@midlight/stores';
  import type { Checkpoint, TiptapDocument } from '@midlight/core/types';
  import SaveSnapshotModal from './SaveSnapshotModal.svelte';
  import RestoreConfirmDialog from './RestoreConfirmDialog.svelte';
  import DiffViewer from './DiffViewer.svelte';

  interface DiffResult {
    additions: string[];
    deletions: string[];
    changeCount: number;
  }

  // Modal states
  let showSaveModal = $state(false);
  let showRestoreDialog = $state(false);
  let checkpointToRestore = $state<Checkpoint | null>(null);

  // Compare mode state
  let showCompare = $state(false);
  let compareResult = $state<DiffResult | null>(null);
  let compareLoading = $state(false);
  let compareCheckpoint = $state<Checkpoint | null>(null);

  // Load checkpoints when the active file changes
  $effect(() => {
    const filePath = $activeFile?.path;
    if (filePath && $fileSystem.rootDir) {
      loadCheckpoints(filePath);
    } else {
      versions.setVersions([]);
    }
  });

  async function loadCheckpoints(filePath: string) {
    const workspaceRoot = $fileSystem.rootDir;
    if (!workspaceRoot) return;

    versions.setIsLoading(true);
    try {
      const checkpoints = await invoke<Checkpoint[]>('get_checkpoints', {
        workspaceRoot,
        filePath,
      });
      versions.setVersions(checkpoints);
    } catch (error) {
      console.error('Failed to load checkpoints:', error);
      versions.setVersions([]);
    } finally {
      versions.setIsLoading(false);
    }
  }

  async function handleSaveSnapshot(label: string, description: string) {
    const workspaceRoot = $fileSystem.rootDir;
    const filePath = $activeFile?.path;
    const content = $fileSystem.editorContent;

    if (!workspaceRoot || !filePath || !content) {
      console.error('Cannot save snapshot: missing data');
      return;
    }

    try {
      await invoke('create_bookmark', {
        workspaceRoot,
        filePath,
        json: content,
        label,
        description: description || null,
      });

      // Refresh the version list
      await loadCheckpoints(filePath);
      showSaveModal = false;
    } catch (error) {
      console.error('Failed to create bookmark:', error);
    }
  }

  function openRestoreDialog(checkpoint: Checkpoint) {
    checkpointToRestore = checkpoint;
    showRestoreDialog = true;
  }

  async function handleRestore(createBackup: boolean) {
    const workspaceRoot = $fileSystem.rootDir;
    const filePath = $activeFile?.path;

    if (!workspaceRoot || !filePath || !checkpointToRestore) {
      console.error('Cannot restore: missing data');
      return;
    }

    try {
      // Optionally create a backup first
      if (createBackup && $fileSystem.editorContent) {
        await invoke('create_bookmark', {
          workspaceRoot,
          filePath,
          json: $fileSystem.editorContent,
          label: 'Before restore',
          description: `Backup created before restoring to "${checkpointToRestore.label || 'auto-saved version'}"`,
        });
      }

      // Restore the checkpoint
      const document = await invoke<TiptapDocument>('restore_checkpoint', {
        workspaceRoot,
        filePath,
        checkpointId: checkpointToRestore.id,
      });

      // Update the editor content
      fileSystem.setEditorContent(document);
      fileSystem.setIsDirty(false);

      // Refresh the version list
      await loadCheckpoints(filePath);

      showRestoreDialog = false;
      checkpointToRestore = null;
      versions.selectVersion(null);
    } catch (error) {
      console.error('Failed to restore checkpoint:', error);
    }
  }

  function formatRelativeTime(timestamp: string): string {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  }

  function formatTrigger(trigger: string): string {
    switch (trigger) {
      case 'file_open': return 'Opened';
      case 'interval': return 'Auto-saved';
      case 'significant_change': return 'Major edit';
      case 'file_close': return 'Closed';
      case 'bookmark': return 'Saved';
      case 'before_restore': return 'Before restore';
      default: return trigger;
    }
  }

  function selectVersion(id: string) {
    if ($versions.selectedVersionId === id) {
      versions.selectVersion(null);
    } else {
      versions.selectVersion(id);
    }
  }

  async function handleCompare(checkpoint: Checkpoint) {
    const workspaceRoot = $fileSystem.rootDir;
    const filePath = $activeFile?.path;

    if (!workspaceRoot || !filePath) {
      console.error('Cannot compare: missing data');
      return;
    }

    compareCheckpoint = checkpoint;
    showCompare = true;
    compareLoading = true;
    compareResult = null;

    try {
      // Compare selected checkpoint with current editor content
      const result = await invoke<DiffResult>('compare_checkpoints', {
        workspaceRoot,
        filePath,
        baseCheckpointId: checkpoint.id,
        compareJson: $fileSystem.editorContent,
      });
      compareResult = result;
    } catch (error) {
      console.error('Failed to compare checkpoints:', error);
      compareResult = null;
    } finally {
      compareLoading = false;
    }
  }

  function closeCompare() {
    showCompare = false;
    compareResult = null;
    compareCheckpoint = null;
    compareLoading = false;
  }
</script>

<div class="h-full flex flex-col">
  <!-- Header -->
  <div class="h-10 border-b border-border flex items-center justify-between px-3">
    <span class="text-sm font-medium">Version History</span>
    <button
      onclick={() => showSaveModal = true}
      disabled={!$activeFile}
      class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed"
      title="Save named version"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/>
        <polyline points="17 21 17 13 7 13 7 21"/>
        <polyline points="7 3 7 8 15 8"/>
      </svg>
    </button>
  </div>

  <!-- Version List -->
  <div class="flex-1 overflow-auto">
    {#if !$activeFile}
      <div class="text-center text-muted-foreground text-sm py-8 px-3">
        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="mx-auto mb-3 opacity-50">
          <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
          <polyline points="14 2 14 8 20 8"/>
        </svg>
        <p>No file selected</p>
        <p class="text-xs mt-1">Open a document to see its version history</p>
      </div>
    {:else if $versions.isLoading}
      <div class="flex items-center justify-center py-8">
        <div class="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
      </div>
    {:else if $versions.versions.length === 0}
      <div class="text-center text-muted-foreground text-sm py-8 px-3">
        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="mx-auto mb-3 opacity-50">
          <circle cx="12" cy="12" r="10"/>
          <polyline points="12 6 12 12 16 14"/>
        </svg>
        <p>No versions yet</p>
        <p class="text-xs mt-1">Versions are created automatically as you edit</p>
        <button
          onclick={() => showSaveModal = true}
          class="mt-4 px-3 py-1.5 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90"
        >
          Save First Version
        </button>
      </div>
    {:else}
      <div class="divide-y divide-border">
        {#each $versions.versions as checkpoint (checkpoint.id)}
          {@const isSelected = $versions.selectedVersionId === checkpoint.id}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            onclick={() => selectVersion(checkpoint.id)}
            class="w-full p-3 text-left hover:bg-accent/50 transition-colors cursor-pointer {isSelected ? 'bg-accent' : ''}"
          >
            <div class="flex items-start justify-between gap-2">
              <div class="flex items-center gap-2 min-w-0">
                {#if checkpoint.type === 'bookmark'}
                  <!-- Bookmark icon -->
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary flex-shrink-0">
                    <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
                  </svg>
                {:else}
                  <!-- Clock icon -->
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground flex-shrink-0">
                    <circle cx="12" cy="12" r="10"/>
                    <polyline points="12 6 12 12 16 14"/>
                  </svg>
                {/if}
                <div class="min-w-0">
                  <div class="text-sm font-medium truncate">
                    {checkpoint.label || formatRelativeTime(checkpoint.timestamp)}
                  </div>
                  <div class="text-xs text-muted-foreground">
                    {checkpoint.label ? formatRelativeTime(checkpoint.timestamp) : formatTrigger(checkpoint.trigger)}
                  </div>
                </div>
              </div>
              <div class="text-xs flex-shrink-0 {checkpoint.stats.changeSize > 0 ? 'text-green-600' : checkpoint.stats.changeSize < 0 ? 'text-red-600' : 'text-muted-foreground'}">
                {checkpoint.stats.changeSize > 0 ? '+' : ''}{checkpoint.stats.changeSize}
              </div>
            </div>

            {#if checkpoint.description}
              <p class="text-xs text-muted-foreground mt-1 truncate pl-6">
                {checkpoint.description}
              </p>
            {/if}

            {#if isSelected}
              <div class="flex gap-2 mt-2 pl-6">
                <button
                  onclick={(e) => { e.stopPropagation(); openRestoreDialog(checkpoint); }}
                  class="px-2 py-1 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90"
                >
                  Restore
                </button>
                <button
                  onclick={(e) => { e.stopPropagation(); handleCompare(checkpoint); }}
                  class="px-2 py-1 text-xs bg-secondary text-secondary-foreground rounded hover:bg-secondary/80"
                >
                  Compare
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<!-- Modals -->
<SaveSnapshotModal
  open={showSaveModal}
  onSave={handleSaveSnapshot}
  onCancel={() => showSaveModal = false}
/>

<RestoreConfirmDialog
  open={showRestoreDialog}
  checkpoint={checkpointToRestore}
  onRestore={handleRestore}
  onCancel={() => { showRestoreDialog = false; checkpointToRestore = null; }}
/>

<!-- Compare View Modal -->
{#if showCompare}
  <div class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center">
    <div class="bg-card border border-border rounded-lg shadow-xl w-[90vw] max-w-4xl h-[80vh] flex flex-col overflow-hidden">
      <DiffViewer
        diff={compareResult}
        baseLabel={compareCheckpoint?.label || formatRelativeTime(compareCheckpoint?.timestamp || '')}
        compareLabel="Current document"
        isLoading={compareLoading}
        onClose={closeCompare}
      />
    </div>
  </div>
{/if}
