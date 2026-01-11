<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { writeText } from '@tauri-apps/plugin-clipboard-manager';
  import { fileSystem, selectedPaths, projectStore, rag } from '@midlight/stores';
  import type { FileNode, ProjectStatus } from '@midlight/core/types';
  import ConfirmDialog from './ConfirmDialog.svelte';

  interface Props {
    x: number;
    y: number;
    targetPath: string;
    targetNode: FileNode | null;
    selectedCount: number;
    onClose: () => void;
    onNewFile: (parentPath: string) => void;
    onNewFolder: (parentPath: string) => void;
    onRename: (path: string) => void;
    onDelete: (paths: string[]) => void;
    onRefresh: () => void;
  }

  let {
    x,
    y,
    targetPath,
    targetNode,
    selectedCount,
    onClose,
    onNewFile,
    onNewFolder,
    onRename,
    onDelete,
    onRefresh,
  }: Props = $props();

  let menuRef: HTMLDivElement | null = $state(null);

  // Archive confirmation state
  let showArchiveConfirm = $state(false);

  const isFolder = $derived(targetNode?.type === 'directory');
  const isMultiSelect = $derived(selectedCount > 1);
  const clipboard = $derived(fileSystem.getClipboard());
  const hasClipboard = $derived(clipboard.paths.length > 0);
  const isProject = $derived(isFolder && projectStore.isProject(targetPath));
  const projectStatus = $derived(isProject ? projectStore.getProjectStatus(targetPath) : null);

  // RAG indexing state
  let isIndexing = $state(false);

  async function handleIndexProject(force = false) {
    if (!isProject) return;

    isIndexing = true;
    try {
      await rag.indexProject(targetPath, force);
    } catch (e) {
      console.error('Failed to index project:', e);
    } finally {
      isIndexing = false;
    }
    onClose();
  }

  // Get all selected paths (or just the target if not in selection)
  function getTargetPaths(): string[] {
    const selected = $selectedPaths;
    if (selected.includes(targetPath)) {
      return selected;
    }
    return [targetPath];
  }

  // Position adjustment to keep menu on screen
  $effect(() => {
    if (menuRef) {
      const rect = menuRef.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;

      if (rect.right > viewportWidth) {
        menuRef.style.left = `${x - rect.width}px`;
      }
      if (rect.bottom > viewportHeight) {
        menuRef.style.top = `${y - rect.height}px`;
      }
    }
  });

  // Close on click outside
  function handleClickOutside(e: MouseEvent) {
    if (menuRef && !menuRef.contains(e.target as Node)) {
      onClose();
    }
  }

  // Close on escape
  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      onClose();
    }
  }

  // Menu actions
  async function handleOpen() {
    if (!isFolder && targetNode) {
      fileSystem.openFile(targetNode);
    }
    onClose();
  }

  function handleNewFile() {
    const parentPath = isFolder ? targetPath : targetPath.substring(0, targetPath.lastIndexOf('/'));
    onNewFile(parentPath);
    onClose();
  }

  function handleNewFolder() {
    const parentPath = isFolder ? targetPath : targetPath.substring(0, targetPath.lastIndexOf('/'));
    onNewFolder(parentPath);
    onClose();
  }

  function handleCopy() {
    const paths = getTargetPaths();
    fileSystem.copyToClipboard(paths);
    onClose();
  }

  function handleCut() {
    const paths = getTargetPaths();
    fileSystem.cutToClipboard(paths);
    onClose();
  }

  async function handlePaste() {
    if (!hasClipboard) return;

    const destDir = isFolder ? targetPath : targetPath.substring(0, targetPath.lastIndexOf('/'));
    const { paths, operation } = clipboard;

    try {
      if (operation === 'copy') {
        await invoke('file_copy_to', { sourcePaths: paths, destDir });
      } else if (operation === 'cut') {
        await invoke('file_move_to', { sourcePaths: paths, destDir });
        fileSystem.clearClipboard();
      }
      onRefresh();
    } catch (e) {
      console.error('Paste failed:', e);
    }
    onClose();
  }

  function handleRename() {
    onRename(targetPath);
    onClose();
  }

  async function handleDuplicate() {
    const paths = getTargetPaths();
    try {
      for (const path of paths) {
        await invoke('file_duplicate', { path });
      }
      onRefresh();
    } catch (e) {
      console.error('Duplicate failed:', e);
    }
    onClose();
  }

  async function handleCopyPath() {
    try {
      await writeText(targetPath);
    } catch (e) {
      // Fallback to navigator clipboard
      await navigator.clipboard.writeText(targetPath);
    }
    onClose();
  }

  async function handleReveal() {
    try {
      await invoke('file_reveal', { path: targetPath });
    } catch (e) {
      console.error('Reveal failed:', e);
    }
    onClose();
  }

  function handleDelete() {
    const paths = getTargetPaths();
    onDelete(paths);
    onClose();
  }

  function handleArchiveClick() {
    // Show confirmation dialog for archiving
    showArchiveConfirm = true;
  }

  async function confirmArchive() {
    showArchiveConfirm = false;
    await applyProjectStatus('archived');
  }

  function cancelArchive() {
    showArchiveConfirm = false;
  }

  async function handleSetProjectStatus(status: ProjectStatus) {
    if (!isProject) return;

    // Show confirmation for archiving
    if (status === 'archived') {
      handleArchiveClick();
      return;
    }

    await applyProjectStatus(status);
  }

  async function applyProjectStatus(status: ProjectStatus) {
    if (!isProject) return;

    try {
      // Update the store
      projectStore.setProjectStatus(targetPath, status);

      // Read current config, update status, and write back
      const configPath = `${targetPath}/.project.midlight`;
      const configContent = await invoke<string>('read_file', { path: configPath });
      const config = JSON.parse(configContent);
      config.status = status;
      await invoke('write_file', { path: configPath, content: JSON.stringify(config, null, 2) });
    } catch (e) {
      console.error('Failed to update project status:', e);
    }
    onClose();
  }
</script>

<svelte:window onclick={handleClickOutside} onkeydown={handleKeyDown} />

<div
  bind:this={menuRef}
  class="fixed bg-popover rounded-lg shadow-xl border border-border py-1 min-w-[180px] z-50"
  style="left: {x}px; top: {y}px;"
  role="menu"
>
  {#if isMultiSelect}
    <!-- Multi-selection header -->
    <div class="px-3 py-2 text-xs text-muted-foreground border-b border-border">
      {selectedCount} items selected
    </div>
  {:else}
    <!-- Single item actions -->
    {#if !isFolder}
      <button
        class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground"
        onclick={handleOpen}
        role="menuitem"
      >
        Open
      </button>
      <div class="h-px bg-border my-1"></div>
    {/if}

    {#if isFolder}
      <button
        class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground"
        onclick={handleNewFile}
        role="menuitem"
      >
        New Document
      </button>
      <button
        class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground"
        onclick={handleNewFolder}
        role="menuitem"
      >
        New Folder
      </button>
      <div class="h-px bg-border my-1"></div>
    {/if}

    {#if isProject}
      {#if projectStatus === 'archived'}
        <!-- Restore option for archived projects -->
        <button
          class="w-full px-3 py-1.5 text-sm text-left text-primary hover:bg-accent hover:text-accent-foreground flex items-center gap-2"
          onclick={() => applyProjectStatus('active')}
          role="menuitem"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/>
            <path d="M3 3v5h5"/>
          </svg>
          Restore Project
        </button>
        <div class="h-px bg-border my-1"></div>
      {:else}
        <!-- Project Status Options -->
        <div class="px-3 py-1 text-xs text-muted-foreground">Project Status</div>
        <button
          class="w-full px-3 py-1.5 text-sm text-left hover:bg-accent hover:text-accent-foreground flex items-center gap-2 {projectStatus === 'active' ? 'text-primary' : 'text-popover-foreground'}"
          onclick={() => handleSetProjectStatus('active')}
          role="menuitem"
        >
          {#if projectStatus === 'active'}
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
          {:else}
            <span class="w-3.5"></span>
          {/if}
          Active
        </button>
        <button
          class="w-full px-3 py-1.5 text-sm text-left hover:bg-accent hover:text-accent-foreground flex items-center gap-2 {projectStatus === 'paused' ? 'text-yellow-500' : 'text-popover-foreground'}"
          onclick={() => handleSetProjectStatus('paused')}
          role="menuitem"
        >
          {#if projectStatus === 'paused'}
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
          {:else}
            <span class="w-3.5"></span>
          {/if}
          Paused
        </button>
        <button
          class="w-full px-3 py-1.5 text-sm text-left hover:bg-accent hover:text-accent-foreground flex items-center gap-2 text-popover-foreground"
          onclick={() => handleSetProjectStatus('archived')}
          role="menuitem"
        >
          <span class="w-3.5"></span>
          Archive
        </button>
        <div class="h-px bg-border my-1"></div>
        <!-- Index for Search option -->
        <button
          class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground flex items-center gap-2"
          onclick={() => handleIndexProject(false)}
          disabled={isIndexing}
          role="menuitem"
        >
          {#if isIndexing}
            <svg class="animate-spin" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
            </svg>
            Indexing...
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8"/>
              <path d="m21 21-4.3-4.3"/>
            </svg>
            Index for Search
          {/if}
        </button>
        <button
          class="w-full px-3 py-1.5 text-sm text-left text-muted-foreground hover:bg-accent hover:text-accent-foreground flex items-center gap-2"
          onclick={() => handleIndexProject(true)}
          disabled={isIndexing}
          role="menuitem"
        >
          <span class="w-3.5"></span>
          Re-index (force)
        </button>
        <div class="h-px bg-border my-1"></div>
      {/if}
    {/if}
  {/if}

  <!-- Common actions for single and multi -->
  <button
    class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground flex justify-between"
    onclick={handleCopy}
    role="menuitem"
  >
    <span>Copy</span>
    <span class="text-muted-foreground text-xs">Cmd+C</span>
  </button>
  <button
    class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground flex justify-between"
    onclick={handleCut}
    role="menuitem"
  >
    <span>Cut</span>
    <span class="text-muted-foreground text-xs">Cmd+X</span>
  </button>
  {#if hasClipboard && (isFolder || !isMultiSelect)}
    <button
      class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground flex justify-between"
      onclick={handlePaste}
      role="menuitem"
    >
      <span>Paste</span>
      <span class="text-muted-foreground text-xs">Cmd+V</span>
    </button>
  {/if}

  <div class="h-px bg-border my-1"></div>

  {#if !isMultiSelect}
    <button
      class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground flex justify-between"
      onclick={handleRename}
      role="menuitem"
    >
      <span>Rename</span>
      <span class="text-muted-foreground text-xs">F2</span>
    </button>
  {/if}
  <button
    class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground"
    onclick={handleDuplicate}
    role="menuitem"
  >
    Duplicate
  </button>

  {#if !isMultiSelect}
    <div class="h-px bg-border my-1"></div>
    <button
      class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground"
      onclick={handleCopyPath}
      role="menuitem"
    >
      Copy Path
    </button>
    <button
      class="w-full px-3 py-1.5 text-sm text-left text-popover-foreground hover:bg-accent hover:text-accent-foreground"
      onclick={handleReveal}
      role="menuitem"
    >
      Reveal in Finder
    </button>
  {/if}

  <div class="h-px bg-border my-1"></div>
  <button
    class="w-full px-3 py-1.5 text-sm text-left text-destructive hover:bg-accent"
    onclick={handleDelete}
    role="menuitem"
  >
    Delete
  </button>
</div>

<!-- Archive Confirmation Dialog -->
<ConfirmDialog
  open={showArchiveConfirm}
  title="Archive Project?"
  message="Archived projects will be hidden from the main view but can be restored at any time. All files will be preserved."
  confirmText="Archive"
  cancelText="Cancel"
  variant="default"
  onConfirm={confirmArchive}
  onCancel={cancelArchive}
/>
