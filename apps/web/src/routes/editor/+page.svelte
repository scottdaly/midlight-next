<script lang="ts">
  import { onMount } from 'svelte';
  import { fileSystem, activeFile } from '@midlight/stores';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Editor from '$lib/components/Editor.svelte';
  import RightSidebar from '$lib/components/RightSidebar.svelte';
  import SyncStatus from '$lib/components/SyncStatus.svelte';

  let sidebarWidth = $state(240);
  let rightSidebarWidth = $state(320);
  let rightSidebarOpen = $state(true);

  // Load default workspace on mount
  onMount(async () => {
    await fileSystem.loadDir('/');
  });
</script>

<div class="flex h-screen overflow-hidden">
  <!-- Left Sidebar - File Tree -->
  <aside
    class="flex-shrink-0 border-r border-border bg-card overflow-hidden"
    style="width: {sidebarWidth}px"
  >
    <Sidebar />
  </aside>

  <!-- Main Editor Area -->
  <main class="flex-1 flex flex-col min-w-0 overflow-hidden">
    <!-- Tab Bar -->
    <div class="h-10 border-b border-border bg-card flex items-center justify-between px-2">
      <div class="flex items-center gap-1">
        {#if $activeFile}
          <div class="flex items-center gap-2 px-3 py-1 bg-background rounded text-sm">
            <span>{$activeFile.name}</span>
            {#if $fileSystem.isDirty}
              <span class="w-2 h-2 bg-primary rounded-full"></span>
            {/if}
          </div>
        {:else}
          <span class="text-muted-foreground text-sm px-3">No file open</span>
        {/if}
      </div>

      <!-- Sync Status in Tab Bar -->
      <SyncStatus />
    </div>

    <!-- Editor -->
    <div class="flex-1 overflow-auto">
      <Editor />
    </div>
  </main>

  <!-- Right Sidebar - Chat/Versions -->
  {#if rightSidebarOpen}
    <aside
      class="flex-shrink-0 border-l border-border bg-card overflow-hidden"
      style="width: {rightSidebarWidth}px"
    >
      <RightSidebar />
    </aside>
  {/if}
</div>

<!-- Toggle Right Sidebar Button -->
<button
  onclick={() => rightSidebarOpen = !rightSidebarOpen}
  class="fixed bottom-4 right-4 p-2 bg-card border border-border rounded-full shadow-lg hover:bg-accent transition-colors z-50"
  title={rightSidebarOpen ? 'Hide AI Panel' : 'Show AI Panel'}
>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    {#if rightSidebarOpen}
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
      <polyline points="17 8 12 3 7 8" />
      <line x1="12" y1="3" x2="12" y2="15" />
    {:else}
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
    {/if}
  </svg>
</button>
