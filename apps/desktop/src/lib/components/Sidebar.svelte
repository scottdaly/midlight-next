<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { fileSystem, activeFile, selectedPaths, settings, pendingNewItem, pendingChanges, projectPaths, workflowStore, projectStore } from '@midlight/stores';
  import type { FileNode, ProjectStatus } from '@midlight/core/types';
  import FileContextMenu from './FileContextMenu.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';

  // Local tree state with expansion tracking
  interface TreeNode extends FileNode {
    expanded?: boolean;
  }

  // Flat node for virtual scrolling - includes depth for indentation
  interface FlatNode extends TreeNode {
    depth: number;
    parentPath: string | null;
  }

  let fileTree = $state<TreeNode[]>([]);
  let expandedPaths = $state<Set<string>>(new Set());

  // Virtual scrolling state
  let scrollContainer: HTMLDivElement | undefined = $state();

  // Pending new item state
  let newItemValue = $state('');
  let newItemInputRef: HTMLInputElement | null = $state(null);

  // Context menu state
  let contextMenu = $state<{
    show: boolean;
    x: number;
    y: number;
    targetPath: string;
    targetNode: FileNode | null;
  }>({
    show: false,
    x: 0,
    y: 0,
    targetPath: '',
    targetNode: null,
  });

  // Rename state
  let renamingPath = $state<string | null>(null);
  let renameValue = $state('');
  let renameInputRef: HTMLInputElement | null = $state(null);

  // Delete confirmation state
  let deleteDialog = $state<{
    show: boolean;
    paths: string[];
  }>({
    show: false,
    paths: [],
  });

  // Drag and drop state
  let draggedPaths = $state<string[]>([]);
  let dropTargetPath = $state<string | null>(null);

  // Keyboard navigation state
  let focusedPath = $state<string | null>(null);

  // New item dropdown state
  let showNewDropdown = $state(false);

  // Build file tree from flat file list
  $effect(() => {
    const files = $fileSystem.files;
    if (!files.length) {
      fileTree = [];
      return;
    }

    // Convert flat list with children already populated from backend
    fileTree = files.map((f) => ({
      ...f,
      expanded: expandedPaths.has(f.path),
    }));
  });

  // Flatten tree for virtual scrolling - returns nodes with depth info
  function flattenVisibleTree(
    nodes: TreeNode[],
    depth = 0,
    parentPath: string | null = null
  ): FlatNode[] {
    const result: FlatNode[] = [];
    for (const node of nodes) {
      result.push({ ...node, depth, parentPath });
      if (node.type === 'directory' && expandedPaths.has(node.path) && node.children) {
        result.push(...flattenVisibleTree(node.children as TreeNode[], depth + 1, node.path));
      }
    }
    return result;
  }

  // Flattened visible nodes for virtual scrolling
  const visibleNodes = $derived(flattenVisibleTree(fileTree));

  // Flat paths for shift-click range selection (just paths, not full nodes)
  const flatPaths = $derived(visibleNodes.map(n => n.path));

  // Virtual scrolling configuration
  const ROW_HEIGHT = 28; // Height of each row in pixels
  const OVERSCAN = 10; // Extra rows to render above/below viewport

  // Create virtualizer - recreate when visibleNodes changes
  const virtualizer = $derived(
    scrollContainer
      ? createVirtualizer({
          count: visibleNodes.length,
          getScrollElement: () => scrollContainer!,
          estimateSize: () => ROW_HEIGHT,
          overscan: OVERSCAN,
        })
      : null
  );

  async function openFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Open Workspace',
    });

    if (selected && typeof selected === 'string') {
      await fileSystem.loadDir(selected);
    }
  }

  async function refresh() {
    await fileSystem.refresh();
  }

  function handleFileClick(node: TreeNode, event: MouseEvent) {
    // Multi-selection handling
    const isMeta = event.metaKey || event.ctrlKey;
    const isShift = event.shiftKey;

    if (isShift) {
      fileSystem.selectFile(node.path, 'range', flatPaths);
    } else if (isMeta) {
      fileSystem.selectFile(node.path, 'toggle');
    } else {
      fileSystem.selectFile(node.path, 'single');
    }

    // Toggle folder expansion on click
    if (node.type === 'directory') {
      toggleExpand(node.path);
    } else if ((node.category === 'midlight' || node.category === 'native') && !isMeta && !isShift) {
      // Open .midlight and .md files on single click
      fileSystem.openFile(node);
    }
  }

  function handleDoubleClick(node: TreeNode) {
    if (node.type === 'file' && (node.category === 'midlight' || node.category === 'native')) {
      fileSystem.openFile(node);
    }
  }

  function toggleExpand(path: string) {
    if (expandedPaths.has(path)) {
      expandedPaths.delete(path);
    } else {
      expandedPaths.add(path);
    }
    expandedPaths = new Set(expandedPaths);
    // Trigger tree rebuild
    fileTree = fileTree.map((n) => ({
      ...n,
      expanded: expandedPaths.has(n.path),
    }));
  }

  function handleContextMenu(event: MouseEvent, node: TreeNode) {
    event.preventDefault();

    // If right-clicking on unselected item, select it
    if (!$selectedPaths.includes(node.path)) {
      fileSystem.selectFile(node.path, 'single');
    }

    contextMenu = {
      show: true,
      x: event.clientX,
      y: event.clientY,
      targetPath: node.path,
      targetNode: node,
    };
  }

  function closeContextMenu() {
    contextMenu = { ...contextMenu, show: false };
  }

  // Keyboard shortcuts
  function handleKeyDown(event: KeyboardEvent) {
    if (renamingPath) return; // Don't handle while renaming

    // Don't handle keyboard shortcuts when focus is in an editable element
    const target = event.target as HTMLElement;
    if (
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      target.closest('[contenteditable="true"]') ||
      target.closest('.tiptap') ||
      target.closest('.ProseMirror')
    ) {
      return;
    }

    const selected = $selectedPaths;
    const isMeta = event.metaKey || event.ctrlKey;

    // Arrow key navigation for file tree
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const currentPath = selected.length > 0 ? selected[selected.length - 1] : focusedPath;
      const currentIndex = currentPath ? visibleNodes.findIndex(n => n.path === currentPath) : -1;

      let nextIndex: number;
      if (event.key === 'ArrowDown') {
        nextIndex = currentIndex < visibleNodes.length - 1 ? currentIndex + 1 : currentIndex;
      } else {
        nextIndex = currentIndex > 0 ? currentIndex - 1 : 0;
      }

      if (nextIndex >= 0 && nextIndex < visibleNodes.length) {
        const nextNode = visibleNodes[nextIndex];
        fileSystem.selectFile(nextNode.path, 'single');
        focusedPath = nextNode.path;

        // Scroll to the item using virtualizer
        if (virtualizer && $virtualizer) {
          $virtualizer.scrollToIndex(nextIndex, { align: 'auto' });
        }
      }
      return;
    }

    // Arrow right/left for expand/collapse
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      if (selected.length === 1) {
        event.preventDefault();
        const node = visibleNodes.find(n => n.path === selected[0]);
        if (node?.type === 'directory') {
          if (event.key === 'ArrowRight' && !expandedPaths.has(node.path)) {
            toggleExpand(node.path);
          } else if (event.key === 'ArrowLeft' && expandedPaths.has(node.path)) {
            toggleExpand(node.path);
          } else if (event.key === 'ArrowLeft' && node.parentPath) {
            // Navigate to parent
            fileSystem.selectFile(node.parentPath, 'single');
            focusedPath = node.parentPath;
          }
        } else if (event.key === 'ArrowLeft' && node?.parentPath) {
          // Navigate to parent for files
          fileSystem.selectFile(node.parentPath, 'single');
          focusedPath = node.parentPath;
        }
      }
      return;
    }

    // Enter to open file or toggle directory
    if (event.key === 'Enter' && selected.length === 1) {
      event.preventDefault();
      const node = visibleNodes.find(n => n.path === selected[0]);
      if (node) {
        if (node.type === 'directory') {
          toggleExpand(node.path);
        } else if (node.category === 'midlight' || node.category === 'native') {
          fileSystem.openFile(node);
        }
      }
      return;
    }

    if (selected.length === 0) return;

    if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      showDeleteDialog(selected);
    } else if (event.key === 'F2' && selected.length === 1) {
      event.preventDefault();
      startRename(selected[0]);
    } else if (isMeta && event.key === 'c') {
      event.preventDefault();
      fileSystem.copyToClipboard(selected);
    } else if (isMeta && event.key === 'x') {
      event.preventDefault();
      fileSystem.cutToClipboard(selected);
    } else if (isMeta && event.key === 'v') {
      event.preventDefault();
      handlePaste();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      fileSystem.clearSelection();
    }
  }

  async function handlePaste() {
    const { paths, operation } = fileSystem.getClipboard();
    if (paths.length === 0) return;

    const selected = $selectedPaths;
    let destDir = $fileSystem.rootDir;

    if (selected.length === 1) {
      const node = findNodeByPath(fileTree, selected[0]);
      if (node?.type === 'directory') {
        destDir = node.path;
      } else if (node) {
        destDir = node.path.substring(0, node.path.lastIndexOf('/'));
      }
    }

    if (!destDir) return;

    try {
      if (operation === 'copy') {
        await invoke('file_copy_to', { sourcePaths: paths, destDir });
      } else if (operation === 'cut') {
        await invoke('file_move_to', { sourcePaths: paths, destDir });
        fileSystem.clearClipboard();
      }
      await refresh();
    } catch (e) {
      console.error('Paste failed:', e);
    }
  }

  function findNodeByPath(nodes: TreeNode[], path: string): TreeNode | null {
    for (const node of nodes) {
      if (node.path === path) return node;
      if (node.children) {
        const found = findNodeByPath(node.children as TreeNode[], path);
        if (found) return found;
      }
    }
    return null;
  }

  // New file/folder creation
  async function createNewFile(parentPath: string) {
    const name = prompt('Enter file name:');
    if (!name) return;

    const fileName = name.endsWith('.md') ? name : `${name}.md`;
    const fullPath = `${parentPath}/${fileName}`;

    try {
      await invoke('write_file', { path: fullPath, content: '' });
      await refresh();
    } catch (e) {
      console.error('Failed to create file:', e);
    }
  }

  async function createNewFolder(parentPath: string) {
    const name = prompt('Enter folder name:');
    if (!name) return;

    const fullPath = `${parentPath}/${name}`;

    try {
      await invoke('create_folder', { path: fullPath });
      await refresh();
    } catch (e) {
      console.error('Failed to create folder:', e);
    }
  }

  // Rename handling
  function startRename(path: string) {
    const node = findNodeByPath(fileTree, path);
    if (!node) return;

    renamingPath = path;
    // Set initial value to filename without extension for files
    if (node.type === 'file') {
      const ext = node.name.lastIndexOf('.');
      renameValue = ext > 0 ? node.name.substring(0, ext) : node.name;
    } else {
      renameValue = node.name;
    }
  }

  $effect(() => {
    if (renamingPath && renameInputRef) {
      renameInputRef.focus();
      renameInputRef.select();
    }
  });

  async function commitRename() {
    if (!renamingPath || !renameValue.trim()) {
      renamingPath = null;
      return;
    }

    const node = findNodeByPath(fileTree, renamingPath);
    if (!node) {
      renamingPath = null;
      return;
    }

    // Build new path
    const parentPath = renamingPath.substring(0, renamingPath.lastIndexOf('/'));
    let newName = renameValue.trim();

    // Preserve extension for files
    if (node.type === 'file') {
      const ext = node.name.lastIndexOf('.');
      if (ext > 0 && !newName.includes('.')) {
        newName = `${newName}${node.name.substring(ext)}`;
      }
    }

    const newPath = `${parentPath}/${newName}`;

    // Check for duplicate
    if (newPath !== renamingPath) {
      try {
        const exists = await invoke('file_exists', { path: newPath });
        if (exists) {
          alert('A file with that name already exists.');
          return;
        }

        await invoke('rename_file', { oldPath: renamingPath, newPath });
        await refresh();
      } catch (e) {
        console.error('Rename failed:', e);
      }
    }

    renamingPath = null;
  }

  function cancelRename() {
    renamingPath = null;
    renameValue = '';
  }

  function handleRenameKeyDown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      commitRename();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      cancelRename();
    }
  }

  // New item handling
  $effect(() => {
    const pending = $pendingNewItem;
    if (pending) {
      newItemValue = pending.defaultName;
      // Focus the input after DOM updates
      requestAnimationFrame(() => {
        if (newItemInputRef) {
          newItemInputRef.focus();
          newItemInputRef.select();
        }
      });
    }
  });

  async function commitNewItem() {
    const pending = $pendingNewItem;
    if (!pending || !newItemValue.trim()) {
      fileSystem.cancelNewItem();
      return;
    }

    await fileSystem.confirmNewItem(newItemValue.trim());
    newItemValue = '';
  }

  function cancelNewItem() {
    fileSystem.cancelNewItem();
    newItemValue = '';
  }

  function handleNewItemKeyDown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      commitNewItem();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      cancelNewItem();
    }
  }

  // Delete handling
  function showDeleteDialog(paths: string[]) {
    deleteDialog = { show: true, paths };
  }

  async function confirmDelete() {
    const { paths } = deleteDialog;
    deleteDialog = { show: false, paths: [] };

    try {
      for (const path of paths) {
        await invoke('file_trash', { path });
      }
      fileSystem.clearSelection();
      await refresh();
    } catch (e) {
      console.error('Delete failed:', e);
    }
  }

  function cancelDelete() {
    deleteDialog = { show: false, paths: [] };
  }

  // Drag and drop handlers
  function handleDragStart(event: DragEvent, node: TreeNode) {
    const selected = $selectedPaths;
    const paths = selected.includes(node.path) ? selected : [node.path];

    draggedPaths = paths;
    event.dataTransfer!.setData('text/plain', JSON.stringify(paths));
    event.dataTransfer!.effectAllowed = 'move';
  }

  function handleDragOver(event: DragEvent, node: TreeNode) {
    if (node.type !== 'directory') return;
    if (draggedPaths.includes(node.path)) return;

    // Check if trying to drop into self or descendant
    for (const dragPath of draggedPaths) {
      if (node.path.startsWith(dragPath + '/') || node.path === dragPath) {
        return;
      }
    }

    event.preventDefault();
    event.dataTransfer!.dropEffect = 'move';
    dropTargetPath = node.path;
  }

  function handleDragLeave(event: DragEvent) {
    const relatedTarget = event.relatedTarget as HTMLElement;
    if (!relatedTarget || !relatedTarget.closest('[data-drop-target]')) {
      dropTargetPath = null;
    }
  }

  async function handleDrop(event: DragEvent, node: TreeNode) {
    event.preventDefault();
    dropTargetPath = null;

    if (node.type !== 'directory') return;

    const data = event.dataTransfer?.getData('text/plain');
    if (!data) return;

    try {
      const paths = JSON.parse(data) as string[];
      await invoke('file_move_to', { sourcePaths: paths, destDir: node.path });
      await refresh();
    } catch (e) {
      console.error('Drop failed:', e);
    }

    draggedPaths = [];
  }

  function handleDragEnd() {
    draggedPaths = [];
    dropTargetPath = null;
  }

  // Check if a path is a project
  function isProject(path: string): boolean {
    // Get relative path from workspace root
    const rootDir = $fileSystem.rootDir || '';
    let relativePath = path;
    if (path.startsWith(rootDir)) {
      relativePath = path.slice(rootDir.length).replace(/^\//, '');
    }
    return $projectPaths.has(relativePath) || $projectPaths.has(path);
  }

  // Get project status for a path
  function getProjectStatus(path: string): ProjectStatus | null {
    if (!isProject(path)) return null;
    return projectStore.getProjectStatus(path);
  }

  // File type icons
  function getFileIcon(node: TreeNode): { icon: string; color: string; isProject?: boolean; projectStatus?: ProjectStatus | null } {
    if (node.type === 'directory') {
      // Check if this directory is a project
      if (isProject(node.path)) {
        const status = getProjectStatus(node.path);
        // Archived projects get muted styling
        if (status === 'archived') {
          return { icon: 'project', color: 'text-muted-foreground opacity-60', isProject: true, projectStatus: status };
        }
        // Paused projects get yellow styling
        if (status === 'paused') {
          return { icon: 'project', color: 'text-yellow-500', isProject: true, projectStatus: status };
        }
        // Active projects (default)
        return { icon: 'project', color: 'text-primary', isProject: true, projectStatus: status };
      }
      return { icon: 'folder', color: 'text-muted-foreground' };
    }

    switch (node.category) {
      case 'midlight':
        return { icon: 'midlight', color: 'text-primary' };
      case 'native':
        return { icon: 'markdown', color: 'text-blue-500' };
      case 'importable':
        return { icon: 'document', color: 'text-orange-500' };
      case 'viewable':
        return { icon: 'image', color: 'text-green-500' };
      default:
        return { icon: 'file', color: 'text-muted-foreground' };
    }
  }

  // Get display name (hide .midlight extension)
  function getDisplayName(node: TreeNode): string {
    if (node.type === 'directory') return node.name;
    if (node.category === 'midlight' && node.name.endsWith('.midlight')) {
      return node.name.slice(0, -9); // Remove '.midlight'
    }
    return node.name;
  }

  // Check if a file has pending changes
  function hasPendingChangesForPath(path: string): boolean {
    return $pendingChanges.some((change) => change.path === path);
  }
</script>

<svelte:window onkeydown={handleKeyDown} onclick={() => showNewDropdown = false} />

<div class="h-full flex flex-col bg-card">
  <!-- Header -->
  <div class="h-10 border-b border-border flex items-center justify-between px-3">
    <span class="text-sm font-medium text-foreground truncate">
      {$fileSystem.rootDir?.split('/').pop() || 'Files'}
    </span>
    <div class="flex gap-1">
      <!-- New item dropdown -->
      <div class="relative">
        <button
          onclick={(e) => { e.stopPropagation(); showNewDropdown = !showNewDropdown; }}
          class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground"
          title="New..."
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="12" y1="5" x2="12" y2="19"/>
            <line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
        </button>
        {#if showNewDropdown}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_interactive_supports_focus -->
          <div
            class="absolute right-0 top-full mt-1 w-36 bg-popover border border-border rounded-md shadow-lg py-1 z-50"
            role="menu"
            onclick={(e) => e.stopPropagation()}
          >
            <button
              role="menuitem"
              onclick={() => {
                showNewDropdown = false;
                fileSystem.startNewFile($fileSystem.rootDir || undefined);
              }}
              class="w-full flex items-center gap-2 px-3 py-1.5 text-sm text-foreground hover:bg-accent"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                <path d="M14 2v6h6"/>
              </svg>
              New File
            </button>
            <button
              role="menuitem"
              onclick={() => {
                showNewDropdown = false;
                fileSystem.startNewFolder($fileSystem.rootDir || undefined);
              }}
              class="w-full flex items-center gap-2 px-3 py-1.5 text-sm text-foreground hover:bg-accent"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
              </svg>
              New Folder
            </button>
            <div class="h-px bg-border my-1"></div>
            <button
              role="menuitem"
              onclick={() => {
                showNewDropdown = false;
                if ($fileSystem.rootDir) {
                  workflowStore.openPicker($fileSystem.rootDir);
                }
              }}
              class="w-full flex items-center gap-2 px-3 py-1.5 text-sm text-foreground hover:bg-accent"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h16Z"/>
                <path d="M12 10v6"/>
                <path d="m9 13 3-3 3 3"/>
              </svg>
              New Project...
            </button>
          </div>
        {/if}
      </div>
      <button
        onclick={openFolder}
        class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground"
        title="Open Folder"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- File Tree with Virtual Scrolling -->
  <div
    bind:this={scrollContainer}
    class="flex-1 overflow-auto"
    onclick={() => fileSystem.clearSelection()}
    onkeydown={(e) => e.key === 'Escape' && fileSystem.clearSelection()}
    role="tree"
    tabindex="0"
  >
    <!-- Pending new item input (at root level) -->
    {#if $pendingNewItem && $pendingNewItem.parentPath === $fileSystem.rootDir}
      <div class="flex items-center gap-2 px-2 py-1 mb-1">
        {#if $pendingNewItem.type === 'folder'}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 text-amber-500">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        {:else}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 text-blue-500">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <path d="M14 2v6h6"/>
          </svg>
        {/if}
        <input
          bind:this={newItemInputRef}
          bind:value={newItemValue}
          onkeydown={handleNewItemKeyDown}
          onblur={commitNewItem}
          class="flex-1 bg-muted text-foreground text-sm px-1 py-0.5 rounded border border-primary focus:border-ring focus:outline-none"
          onclick={(e) => e.stopPropagation()}
          placeholder={$pendingNewItem.type === 'folder' ? 'New Folder' : 'Untitled'}
        />
      </div>
    {/if}

    <!-- Virtualized file tree -->
    {#if virtualizer && $virtualizer}
      {@const virt = $virtualizer}
      <div
        style="height: {virt.getTotalSize()}px; width: 100%; position: relative;"
      >
        {#each virt.getVirtualItems() as row (visibleNodes[row.index]?.path ?? row.index)}
          {@const node = visibleNodes[row.index]}
          {#if node}
            <div
              style="
                position: absolute;
                top: 0;
                left: 0;
                width: 100%;
                height: {row.size}px;
                transform: translateY({row.start}px);
              "
            >
              {@render fileNodeRow(node)}
            </div>
          {/if}
        {/each}
      </div>
    {:else}
      <!-- Fallback for when virtualizer isn't ready -->
      <div class="p-2">
        {#each visibleNodes as node (node.path)}
          {@render fileNodeRow(node)}
        {/each}
      </div>
    {/if}
  </div>

  <!-- Footer with Settings -->
  <div class="border-t border-border p-2">
    <button
      onclick={() => settings.open()}
      class="w-full flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path>
        <circle cx="12" cy="12" r="3"></circle>
      </svg>
      Settings
    </button>
  </div>
</div>

<!-- Context Menu -->
{#if contextMenu.show}
  <FileContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    targetPath={contextMenu.targetPath}
    targetNode={contextMenu.targetNode}
    selectedCount={$selectedPaths.length}
    onClose={closeContextMenu}
    onNewFile={createNewFile}
    onNewFolder={createNewFolder}
    onRename={startRename}
    onDelete={showDeleteDialog}
    onRefresh={refresh}
  />
{/if}

<!-- Delete Confirmation -->
<ConfirmDialog
  open={deleteDialog.show}
  title="Delete {deleteDialog.paths.length === 1 ? 'item' : `${deleteDialog.paths.length} items`}?"
  message="This will move {deleteDialog.paths.length === 1 ? 'this item' : 'these items'} to the trash."
  confirmText="Delete"
  cancelText="Cancel"
  variant="danger"
  onConfirm={confirmDelete}
  onCancel={cancelDelete}
/>

{#snippet fileNodeRow(node: FlatNode)}
  {@const iconInfo = getFileIcon(node)}
  {@const isSelected = $selectedPaths.includes(node.path)}
  {@const isDropTarget = dropTargetPath === node.path}
  {@const isDragged = draggedPaths.includes(node.path)}
  {@const isExpanded = expandedPaths.has(node.path)}

  <div
    class="group"
    role="treeitem"
    aria-selected={isSelected}
    data-path={node.path}
  >
    {#if renamingPath === node.path}
      <!-- Rename input -->
      <div
        class="flex items-center gap-2 px-2 h-7"
        style="padding-left: {node.depth * 12 + 8}px"
      >
        {#if node.type === 'directory'}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 text-muted-foreground ml-[18px]">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        {:else}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 {iconInfo.color} ml-[18px]">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <path d="M14 2v6h6"/>
          </svg>
        {/if}
        <input
          bind:this={renameInputRef}
          bind:value={renameValue}
          onkeydown={handleRenameKeyDown}
          onblur={commitRename}
          class="flex-1 bg-muted text-foreground text-sm px-1 py-0.5 rounded border border-input focus:border-ring focus:outline-none"
          onclick={(e) => e.stopPropagation()}
        />
      </div>
    {:else}
      <!-- Normal file row -->
      <button
        draggable="true"
        data-drop-target={node.type === 'directory'}
        onclick={(e) => { e.stopPropagation(); handleFileClick(node, e); }}
        ondblclick={() => handleDoubleClick(node)}
        oncontextmenu={(e) => handleContextMenu(e, node)}
        ondragstart={(e) => handleDragStart(e, node)}
        ondragover={(e) => handleDragOver(e, node)}
        ondragleave={handleDragLeave}
        ondrop={(e) => handleDrop(e, node)}
        ondragend={handleDragEnd}
        class="w-full flex items-center gap-2 px-2 h-7 text-sm rounded text-left transition-colors
          {isSelected ? 'bg-primary/30 text-foreground' : 'text-muted-foreground hover:bg-accent hover:text-foreground'}
          {isDropTarget ? 'ring-2 ring-primary bg-primary/20' : ''}
          {isDragged ? 'opacity-50' : ''}"
        style="padding-left: {node.depth * 12 + 8}px"
      >
        {#if node.type === 'directory'}
          <!-- Expand/collapse chevron -->
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
            class="flex-shrink-0 transition-transform text-muted-foreground {isExpanded ? 'rotate-90' : ''}"
          >
            <polyline points="9 18 15 12 9 6"/>
          </svg>
          <!-- Folder or Project icon -->
          {#if iconInfo.isProject}
            <!-- Project briefcase icon with status-based color -->
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 {iconInfo.color}">
              <rect width="20" height="14" x="2" y="7" rx="2" ry="2"/>
              <path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16"/>
            </svg>
          {:else}
            <!-- Regular folder icon -->
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 text-amber-500">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
            </svg>
          {/if}
        {:else}
          <!-- File icon with category-specific color -->
          {#if iconInfo.icon === 'midlight'}
            <!-- Midlight document icon -->
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 {iconInfo.color} ml-[18px]">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/>
              <path d="M14 2v4a2 2 0 0 0 2 2h4"/>
              <path d="M8 12h8"/>
              <path d="M8 16h5"/>
            </svg>
          {:else if iconInfo.icon === 'markdown'}
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 {iconInfo.color} ml-[18px]">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <path d="M14 2v6h6"/>
              <path d="M7 13l3 3 3-3"/>
              <path d="M10 16V10"/>
            </svg>
          {:else if iconInfo.icon === 'image'}
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 {iconInfo.color} ml-[18px]">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
              <circle cx="8.5" cy="8.5" r="1.5"/>
              <polyline points="21 15 16 10 5 21"/>
            </svg>
          {:else if iconInfo.icon === 'document'}
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 {iconInfo.color} ml-[18px]">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <path d="M14 2v6h6"/>
              <line x1="16" y1="13" x2="8" y2="13"/>
              <line x1="16" y1="17" x2="8" y2="17"/>
              <line x1="10" y1="9" x2="8" y2="9"/>
            </svg>
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="flex-shrink-0 {iconInfo.color} ml-[18px]">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <path d="M14 2v6h6"/>
            </svg>
          {/if}
        {/if}
        <span class="truncate {iconInfo.projectStatus === 'archived' ? 'opacity-60' : ''}">{getDisplayName(node)}</span>
        {#if iconInfo.isProject && iconInfo.projectStatus === 'paused'}
          <span class="flex-shrink-0 text-xs text-yellow-500" title="Paused project">⏸</span>
        {:else if iconInfo.isProject && iconInfo.projectStatus === 'archived'}
          <span class="flex-shrink-0 text-xs text-muted-foreground" title="Archived project">📦</span>
        {/if}
        {#if node.type === 'file' && hasPendingChangesForPath(node.path)}
          <span class="flex-shrink-0 w-2 h-2 rounded-full bg-yellow-500 ml-1" title="Pending AI changes"></span>
        {/if}
      </button>
    {/if}
  </div>
{/snippet}
