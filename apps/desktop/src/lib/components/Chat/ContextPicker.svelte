<script lang="ts">
  import { fileSystem } from '@midlight/stores';
  import type { FileNode } from '@midlight/core/types';

  interface Props {
    query: string;
    onSelect: (file: FileNode) => void;
    onClose: () => void;
  }

  let { query, onSelect, onClose }: Props = $props();

  let selectedIndex = $state(0);

  // Flatten file tree for searching
  function flattenFiles(files: FileNode[], result: FileNode[] = []): FileNode[] {
    for (const file of files) {
      if (file.type === 'file') {
        result.push(file);
      } else if (file.children) {
        flattenFiles(file.children, result);
      }
    }
    return result;
  }

  // Filter files based on query
  const filteredFiles = $derived(() => {
    const allFiles = flattenFiles($fileSystem.files);
    if (!query) return allFiles.slice(0, 10);

    const lowerQuery = query.toLowerCase();
    return allFiles
      .filter(f => f.name.toLowerCase().includes(lowerQuery))
      .slice(0, 10);
  });

  // Reset selected index when filtered files change
  $effect(() => {
    const files = filteredFiles();
    if (selectedIndex >= files.length) {
      selectedIndex = Math.max(0, files.length - 1);
    }
  });

  // Get display name (without .midlight extension)
  function getDisplayName(file: FileNode): string {
    if (file.name.endsWith('.midlight')) {
      return file.name.slice(0, -9);
    }
    return file.name;
  }

  // Get relative path for display
  function getRelativePath(file: FileNode): string {
    if (!$fileSystem.rootDir) return file.path;
    return file.path.replace($fileSystem.rootDir, '').replace(/^\//, '');
  }

  // Handle keyboard navigation
  export function handleKeyDown(e: KeyboardEvent): boolean {
    const files = filteredFiles();

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        selectedIndex = Math.min(selectedIndex + 1, files.length - 1);
        return true;
      case 'ArrowUp':
        e.preventDefault();
        selectedIndex = Math.max(selectedIndex - 1, 0);
        return true;
      case 'Enter':
        e.preventDefault();
        if (files[selectedIndex]) {
          onSelect(files[selectedIndex]);
        }
        return true;
      case 'Escape':
        e.preventDefault();
        onClose();
        return true;
      case 'Tab':
        e.preventDefault();
        if (files[selectedIndex]) {
          onSelect(files[selectedIndex]);
        }
        return true;
      default:
        return false;
    }
  }
</script>

<div class="absolute bottom-full left-0 mb-1 w-80 max-h-64 overflow-auto bg-popover border border-border rounded-lg shadow-lg z-50">
  {#if filteredFiles().length === 0}
    <div class="p-3 text-sm text-muted-foreground text-center">
      No files found
    </div>
  {:else}
    <div class="py-1">
      <div class="px-3 py-1.5 text-xs text-muted-foreground uppercase tracking-wide">
        Files
      </div>
      {#each filteredFiles() as file, i}
        <button
          onclick={() => onSelect(file)}
          class="w-full text-left px-3 py-2 flex items-center gap-2 hover:bg-accent {i === selectedIndex ? 'bg-accent' : ''}"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground flex-shrink-0">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <path d="M14 2v6h6"/>
          </svg>
          <div class="min-w-0 flex-1">
            <div class="text-sm font-medium truncate">{getDisplayName(file)}</div>
            <div class="text-xs text-muted-foreground truncate">{getRelativePath(file)}</div>
          </div>
        </button>
      {/each}
    </div>
  {/if}
  <div class="border-t border-border px-3 py-2 text-xs text-muted-foreground flex gap-2">
    <span><kbd class="px-1 bg-muted rounded">↑↓</kbd> navigate</span>
    <span><kbd class="px-1 bg-muted rounded">Enter</kbd> select</span>
    <span><kbd class="px-1 bg-muted rounded">Esc</kbd> close</span>
  </div>
</div>
