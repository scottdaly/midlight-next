<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { fileSystem, projects, activeProjects, pausedProjects, rag, isSearching as ragIsSearching, searchResults as ragSearchResults } from '@midlight/stores';
  import type { FileNode } from '@midlight/core/types';
  import type { SearchResult } from '@midlight/core';
  import type { ProjectInfo } from '@midlight/stores';

  export interface SelectedFile {
    file: FileNode;
    projectPath?: string;
    projectName?: string;
    semanticContent?: string;
    semanticScore?: number;
  }

  interface Props {
    query: string;
    onSelect: (selection: SelectedFile) => void;
    onClose: () => void;
  }

  let { query, onSelect, onClose }: Props = $props();

  let selectedIndex = $state(0);

  // State for browsing other projects
  let browsingProject = $state<ProjectInfo | null>(null);
  let projectFiles = $state<FileNode[]>([]);
  let isLoadingProjectFiles = $state(false);

  // Debounced semantic search
  let searchTimeout: ReturnType<typeof setTimeout> | null = null;
  let lastSemanticQuery = $state('');

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

  // Get current workspace files
  const currentWorkspaceFiles = $derived(() => {
    const allFiles = flattenFiles($fileSystem.files);
    if (!query) return allFiles.slice(0, 8);

    const lowerQuery = query.toLowerCase();
    return allFiles
      .filter(f => f.name.toLowerCase().includes(lowerQuery))
      .slice(0, 8);
  });

  // Get other projects (not the current workspace)
  const otherProjects = $derived(() => {
    const currentRoot = $fileSystem.rootDir;
    // Show active and paused projects, not archived
    return [...$activeProjects, ...$pausedProjects].filter(
      p => p.path !== currentRoot
    );
  });

  // Filter project files when browsing a project
  const filteredProjectFiles = $derived(() => {
    if (!browsingProject) return [];

    const allFiles = flattenFiles(projectFiles);
    if (!query) return allFiles.slice(0, 10);

    const lowerQuery = query.toLowerCase();
    return allFiles
      .filter(f => f.name.toLowerCase().includes(lowerQuery))
      .slice(0, 10);
  });

  // Trigger semantic search when query is long enough
  $effect(() => {
    if (query.length >= 3 && query !== lastSemanticQuery && !browsingProject) {
      // Clear previous timeout
      if (searchTimeout) {
        clearTimeout(searchTimeout);
      }
      // Debounce: wait 300ms before searching
      searchTimeout = setTimeout(async () => {
        lastSemanticQuery = query;
        await rag.search(query, { topK: 5, minScore: 0.5 });
      }, 300);
    }
    return () => {
      if (searchTimeout) {
        clearTimeout(searchTimeout);
      }
    };
  });

  // Transform semantic results for display
  const semanticResults = $derived(() => {
    if (!$ragSearchResults || $ragSearchResults.length === 0) return [];
    return $ragSearchResults.slice(0, 5);
  });

  // Combined items for keyboard navigation
  const allItems = $derived(() => {
    if (browsingProject) {
      return filteredProjectFiles();
    }

    const items: (FileNode | ProjectInfo | SearchResult)[] = [];
    items.push(...currentWorkspaceFiles());
    // Add semantic results if we have a query
    if (query.length >= 3) {
      items.push(...semanticResults());
    }
    items.push(...otherProjects());
    return items;
  });

  // Check if item is a semantic result
  function isSemanticResult(item: FileNode | ProjectInfo | SearchResult): item is SearchResult {
    return 'score' in item && 'content' in item;
  }

  // Reset selected index when items change
  $effect(() => {
    const items = allItems();
    if (selectedIndex >= items.length) {
      selectedIndex = Math.max(0, items.length - 1);
    }
  });

  // Load files for a project
  async function loadProjectFiles(project: ProjectInfo) {
    isLoadingProjectFiles = true;
    try {
      // Use Tauri to load the project's file tree
      const files = await invoke<FileNode[]>('list_files', { path: project.path });
      projectFiles = files;
      browsingProject = project;
      selectedIndex = 0;
    } catch (error) {
      console.error('Failed to load project files:', error);
    } finally {
      isLoadingProjectFiles = false;
    }
  }

  // Go back to project list
  function goBack() {
    browsingProject = null;
    projectFiles = [];
    selectedIndex = 0;
  }

  // Get display name (without .midlight extension)
  function getDisplayName(file: FileNode): string {
    if (file.name.endsWith('.midlight')) {
      return file.name.slice(0, -9);
    }
    return file.name;
  }

  // Get relative path for display
  function getRelativePath(file: FileNode, rootDir?: string): string {
    const root = rootDir || $fileSystem.rootDir;
    if (!root) return file.path;
    return file.path.replace(root, '').replace(/^\//, '');
  }

  // Check if item is a project
  function isProject(item: FileNode | ProjectInfo): item is ProjectInfo {
    return 'config' in item;
  }

  // Handle item selection
  function handleItemClick(item: FileNode | ProjectInfo | SearchResult) {
    if (isProject(item)) {
      loadProjectFiles(item);
    } else if (isSemanticResult(item)) {
      // Create a synthetic file node from semantic result
      const filePath = item.filePath;
      const fileName = filePath.split('/').pop() || filePath;
      onSelect({
        file: {
          name: fileName,
          path: filePath,
          type: 'file',
          category: 'midlight',
        } as FileNode,
        projectPath: item.projectPath,
        projectName: item.projectName,
        semanticContent: item.content,
        semanticScore: item.score,
      });
    } else {
      onSelect({
        file: item,
        projectPath: browsingProject?.path,
        projectName: browsingProject?.config.name,
      });
    }
  }

  // Handle keyboard navigation
  export function handleKeyDown(e: KeyboardEvent): boolean {
    const items = allItems();

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        selectedIndex = Math.min(selectedIndex + 1, items.length - 1);
        return true;
      case 'ArrowUp':
        e.preventDefault();
        selectedIndex = Math.max(selectedIndex - 1, 0);
        return true;
      case 'Enter':
        e.preventDefault();
        if (items[selectedIndex]) {
          handleItemClick(items[selectedIndex]);
        }
        return true;
      case 'Escape':
        e.preventDefault();
        if (browsingProject) {
          goBack();
        } else {
          onClose();
        }
        return true;
      case 'Tab':
        e.preventDefault();
        if (items[selectedIndex] && !isProject(items[selectedIndex])) {
          handleItemClick(items[selectedIndex]);
        }
        return true;
      case 'Backspace':
        if (browsingProject && !query) {
          e.preventDefault();
          goBack();
          return true;
        }
        return false;
      default:
        return false;
    }
  }
</script>

<div class="absolute bottom-full left-0 mb-1 w-96 max-h-80 overflow-auto bg-popover border border-border rounded-lg shadow-lg z-50">
  {#if browsingProject}
    <!-- Browsing inside a project -->
    <div class="sticky top-0 bg-popover border-b border-border px-3 py-2">
      <button onclick={goBack} class="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M19 12H5"/>
          <path d="M12 19l-7-7 7-7"/>
        </svg>
        <span>Back to projects</span>
      </button>
      <div class="mt-1 text-sm font-medium text-foreground">{browsingProject.config.name}</div>
    </div>

    {#if isLoadingProjectFiles}
      <div class="p-3 text-sm text-muted-foreground text-center">
        Loading files...
      </div>
    {:else if filteredProjectFiles().length === 0}
      <div class="p-3 text-sm text-muted-foreground text-center">
        No files found
      </div>
    {:else}
      <div class="py-1">
        {#each filteredProjectFiles() as file, i}
          {@const globalIndex = i}
          <button
            onclick={() => handleItemClick(file)}
            class="w-full text-left px-3 py-2 flex items-center gap-2 hover:bg-accent {globalIndex === selectedIndex ? 'bg-accent' : ''}"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground flex-shrink-0">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <path d="M14 2v6h6"/>
            </svg>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-medium truncate">{getDisplayName(file)}</div>
              <div class="text-xs text-muted-foreground truncate">{getRelativePath(file, browsingProject.path)}</div>
            </div>
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <!-- Main view: current files + other projects -->
    {#if currentWorkspaceFiles().length === 0 && otherProjects().length === 0}
      <div class="p-3 text-sm text-muted-foreground text-center">
        No files or projects found
      </div>
    {:else}
      <div class="py-1">
        <!-- Current workspace files -->
        {#if currentWorkspaceFiles().length > 0}
          <div class="px-3 py-1.5 text-xs text-muted-foreground uppercase tracking-wide">
            Current Workspace
          </div>
          {#each currentWorkspaceFiles() as file, i}
            <button
              onclick={() => handleItemClick(file)}
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
        {/if}

        <!-- Semantic search results -->
        {#if query.length >= 3}
          <div class="h-px bg-border my-1"></div>
          <div class="px-3 py-1.5 text-xs text-muted-foreground uppercase tracking-wide flex items-center gap-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8"/>
              <path d="m21 21-4.3-4.3"/>
            </svg>
            Semantic Search
            {#if $ragIsSearching}
              <svg class="animate-spin w-3 h-3" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
              </svg>
            {/if}
          </div>
          {#if semanticResults().length === 0 && !$ragIsSearching}
            <div class="px-3 py-2 text-sm text-muted-foreground">
              No semantic matches found
            </div>
          {:else}
            {#each semanticResults() as result, i}
              {@const globalIndex = currentWorkspaceFiles().length + i}
              {@const fileName = result.filePath.split('/').pop() || result.filePath}
              {@const displayName = fileName.endsWith('.midlight') ? fileName.slice(0, -9) : fileName}
              <button
                onclick={() => handleItemClick(result)}
                class="w-full text-left px-3 py-2 flex items-start gap-2 hover:bg-accent {globalIndex === selectedIndex ? 'bg-accent' : ''}"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-purple-500 flex-shrink-0 mt-0.5">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                  <path d="M14 2v6h6"/>
                </svg>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-medium truncate">{displayName}</span>
                    {#if result.projectName}
                      <span class="text-xs px-1.5 py-0.5 rounded bg-primary/20 text-primary truncate max-w-24">
                        {result.projectName}
                      </span>
                    {/if}
                    <span class="text-xs text-muted-foreground ml-auto flex-shrink-0">
                      {Math.round(result.score * 100)}%
                    </span>
                  </div>
                  <div class="text-xs text-muted-foreground line-clamp-2 mt-0.5">
                    {result.content.slice(0, 150)}{result.content.length > 150 ? '...' : ''}
                  </div>
                </div>
              </button>
            {/each}
          {/if}
        {/if}

        <!-- Other projects -->
        {#if otherProjects().length > 0}
          <div class="h-px bg-border my-1"></div>
          <div class="px-3 py-1.5 text-xs text-muted-foreground uppercase tracking-wide">
            Other Projects
          </div>
          {#each otherProjects() as project, i}
            {@const globalIndex = currentWorkspaceFiles().length + semanticResults().length + i}
            <button
              onclick={() => handleItemClick(project)}
              class="w-full text-left px-3 py-2 flex items-center gap-2 hover:bg-accent {globalIndex === selectedIndex ? 'bg-accent' : ''}"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary flex-shrink-0">
                <rect width="20" height="14" x="2" y="7" rx="2" ry="2"/>
                <path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16"/>
              </svg>
              <div class="min-w-0 flex-1">
                <div class="text-sm font-medium truncate">{project.config.name}</div>
                <div class="text-xs text-muted-foreground truncate flex items-center gap-1">
                  {project.path.split('/').pop()}
                  <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M9 18l6-6-6-6"/>
                  </svg>
                </div>
              </div>
            </button>
          {/each}
        {/if}
      </div>
    {/if}
  {/if}

  <div class="border-t border-border px-3 py-2 text-xs text-muted-foreground flex gap-2">
    <span><kbd class="px-1 bg-muted rounded">↑↓</kbd> navigate</span>
    <span><kbd class="px-1 bg-muted rounded">Enter</kbd> select</span>
    {#if browsingProject}
      <span><kbd class="px-1 bg-muted rounded">←</kbd> back</span>
    {/if}
    <span><kbd class="px-1 bg-muted rounded">Esc</kbd> close</span>
  </div>
</div>
