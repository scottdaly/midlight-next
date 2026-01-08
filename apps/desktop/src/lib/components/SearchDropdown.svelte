<script lang="ts">
  import { fileSystem } from '@midlight/stores';
  import type { FileNode } from '@midlight/core/types';

  interface Props {
    open: boolean;
    query: string;
    onClose: () => void;
    inputElement: HTMLInputElement | null;
  }

  let { open, query, onClose, inputElement }: Props = $props();

  let selectedIndex = $state(0);
  let listRef: HTMLDivElement | null = $state(null);

  // Flatten file tree to get all files
  function flattenFiles(nodes: FileNode[]): FileNode[] {
    const result: FileNode[] = [];
    for (const node of nodes) {
      if (node.type === 'file') {
        result.push(node);
      }
      if (node.children) {
        result.push(...flattenFiles(node.children));
      }
    }
    return result;
  }

  // Derived state for files
  const allFiles = $derived(flattenFiles($fileSystem.files));

  // Search result with score for ranking
  interface SearchResult {
    file: FileNode;
    score: number;
    matchStart: number;
  }

  const filteredFiles = $derived.by(() => {
    if (!query.trim()) {
      return allFiles.slice(0, 15);
    }
    const lowerQuery = query.toLowerCase();

    // Score and filter files
    const results: SearchResult[] = [];
    for (const file of allFiles) {
      const name = file.name.toLowerCase();
      const index = name.indexOf(lowerQuery);

      if (index === -1) continue;

      // Score: prefer matches at start, then word boundaries
      let score = 100 - index; // Earlier = better
      if (index === 0) score += 50; // Starts with query
      if (index > 0) {
        const prevChar = name[index - 1];
        if (prevChar === '-' || prevChar === '_' || prevChar === '.') {
          score += 25; // Word boundary
        }
      }
      // Boost exact name matches
      if (name === lowerQuery || name.replace(/\.(md|midlight)$/, '') === lowerQuery) {
        score += 100;
      }

      results.push({ file, score, matchStart: index });
    }

    // Sort by score (highest first) and return files
    return results
      .sort((a, b) => b.score - a.score)
      .slice(0, 15)
      .map(r => r.file);
  });

  // Reset selection when results change
  $effect(() => {
    // Access filteredFiles to track dependency
    filteredFiles;
    selectedIndex = 0;
  });

  // Scroll selected item into view
  $effect(() => {
    if (listRef && open) {
      const selectedElement = listRef.querySelector(`[data-index="${selectedIndex}"]`);
      selectedElement?.scrollIntoView({ block: 'nearest' });
    }
  });

  function handleSelect(file: FileNode) {
    fileSystem.openFile(file);
    onClose();
  }

  // Get relative path from root
  function getRelativePath(fullPath: string, rootDir: string | null): string {
    if (!rootDir) return fullPath;
    if (fullPath.startsWith(rootDir)) {
      return fullPath.slice(rootDir.length + 1);
    }
    return fullPath;
  }

  function getFolder(file: FileNode): string {
    const rootDir = $fileSystem.rootDir;
    const relativePath = getRelativePath(file.path, rootDir);
    const pathParts = relativePath.split(/[/\\]/);
    pathParts.pop(); // remove filename
    return pathParts.join('/');
  }

  // Handle keyboard navigation from the input
  $effect(() => {
    if (!open || !inputElement) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Only handle navigation keys
      if (!['ArrowDown', 'ArrowUp', 'Enter'].includes(e.key)) return;

      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          selectedIndex = Math.min(selectedIndex + 1, filteredFiles.length - 1);
          break;
        case 'ArrowUp':
          e.preventDefault();
          selectedIndex = Math.max(selectedIndex - 1, 0);
          break;
        case 'Enter':
          e.preventDefault();
          if (filteredFiles[selectedIndex]) {
            handleSelect(filteredFiles[selectedIndex]);
          }
          break;
      }
    };

    inputElement.addEventListener('keydown', handleKeyDown);
    return () => inputElement.removeEventListener('keydown', handleKeyDown);
  });

  // Icon helper
  function getIcon(category: string | undefined) {
    switch (category) {
      case 'midlight':
      case 'native':
      case 'compatible':
        // File text
        return 'text';
      case 'viewable':
        // Image
        return 'image';
      default:
        // Generic file
        return 'file';
    }
  }
</script>

{#if open}
  <div class="absolute top-full left-0 right-0 mt-1 z-50 px-2 w-[400px] left-1/2 -translate-x-1/2">
    <div class="bg-popover border border-border rounded-lg shadow-xl overflow-hidden">
      <!-- Results list -->
      <div bind:this={listRef} class="max-h-72 overflow-y-auto">
        {#if filteredFiles.length === 0}
          <div class="px-3 py-6 text-center text-sm text-muted-foreground">
            {query ? 'No files found' : 'No files in workspace'}
          </div>
        {:else}
          {#each filteredFiles as file, index}
            {@const isSelected = index === selectedIndex}
            {@const iconType = getIcon(file.category)}
            {@const folderPath = getFolder(file)}

            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              data-index={index}
              onclick={() => handleSelect(file)}
              class="flex items-center gap-3 px-3 py-2 cursor-pointer transition-colors {isSelected ? 'bg-accent text-accent-foreground' : 'hover:bg-muted/50'}"
            >
              <!-- Icon -->
              {#if iconType === 'text'}
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shrink-0 text-muted-foreground"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><line x1="10" y1="9" x2="8" y2="9"/></svg>
              {:else if iconType === 'image'}
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shrink-0 text-muted-foreground"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
              {:else}
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shrink-0 text-muted-foreground"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>
              {/if}

              <div class="flex-1 min-w-0">
                <div class="truncate text-sm">{file.name}</div>
                {#if folderPath}
                  <div class="flex items-center gap-1 text-xs text-muted-foreground truncate">
                    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shrink-0"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
                    <span>{folderPath}</span>
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- Footer hint -->
      <div class="px-3 py-1.5 border-t border-border/50 text-xs text-muted-foreground flex items-center gap-3 bg-muted/30">
        <span><kbd class="px-1 py-0.5 bg-background border border-border rounded text-[10px]">↑↓</kbd> Navigate</span>
        <span><kbd class="px-1 py-0.5 bg-background border border-border rounded text-[10px]">↵</kbd> Open</span>
        <span><kbd class="px-1 py-0.5 bg-background border border-border rounded text-[10px]">Esc</kbd> Close</span>
      </div>
    </div>
  </div>
{/if}
