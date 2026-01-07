<script lang="ts">
  import { fileSystem, activeFile, activeFileIndex } from '@midlight/stores';
  import type { FileNode } from '@midlight/core/types';

  interface Props {
    onOpenFolder?: () => void;
  }

  let { onOpenFolder }: Props = $props();

  let scrollContainer: HTMLDivElement | null = $state(null);
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);
  let showNewMenu = $state(false);
  let newMenuRef: HTMLDivElement | null = $state(null);

  const openFiles = $derived($fileSystem.openFiles);
  const activeIndex = $derived($activeFileIndex);
  const isDirty = $derived($fileSystem.isDirty);
  const activePath = $derived($activeFile?.path);

  function handleNewFile() {
    showNewMenu = false;
    fileSystem.startNewFile();
  }

  function handleNewFolder() {
    showNewMenu = false;
    fileSystem.startNewFolder();
  }

  function toggleNewMenu() {
    showNewMenu = !showNewMenu;
  }

  // Close menu when clicking outside
  function handleClickOutside(e: MouseEvent) {
    if (showNewMenu && newMenuRef && !newMenuRef.contains(e.target as Node)) {
      showNewMenu = false;
    }
  }

  $effect(() => {
    if (showNewMenu) {
      document.addEventListener('click', handleClickOutside, true);
      return () => document.removeEventListener('click', handleClickOutside, true);
    }
  });

  function handleTabClick(index: number) {
    fileSystem.setActiveTab(index);
  }

  function handleClose(e: MouseEvent, path: string) {
    e.stopPropagation();
    fileSystem.closeFile(path);
  }

  function scrollLeft() {
    scrollContainer?.scrollBy({ left: -200, behavior: 'smooth' });
  }

  function scrollRight() {
    scrollContainer?.scrollBy({ left: 200, behavior: 'smooth' });
  }

  function updateScrollState() {
    if (!scrollContainer) return;
    canScrollLeft = scrollContainer.scrollLeft > 0;
    canScrollRight = scrollContainer.scrollLeft < scrollContainer.scrollWidth - scrollContainer.clientWidth - 1;
  }

  // Watch for scroll container changes
  $effect(() => {
    if (scrollContainer) {
      updateScrollState();
      // Also update on resize
      const observer = new ResizeObserver(updateScrollState);
      observer.observe(scrollContainer);
      return () => observer.disconnect();
    }
  });

  // Update scroll state when openFiles changes
  $effect(() => {
    if (openFiles.length >= 0) {
      // Trigger re-check after DOM updates
      requestAnimationFrame(updateScrollState);
    }
  });

  function getFileName(path: string): string {
    let name = path.split('/').pop() || path;
    // Hide .midlight extension from users
    if (name.endsWith('.midlight')) {
      name = name.slice(0, -9);
    }
    return name;
  }
</script>

<div class="h-9 bg-secondary border-b border-border flex items-center px-2 shrink-0">
    <!-- Scroll left button -->
    {#if canScrollLeft}
      <button
        onclick={scrollLeft}
        class="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground shrink-0"
        title="Scroll left"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="m15 18-6-6 6-6"/>
        </svg>
      </button>
    {/if}

    <!-- Tabs container -->
    <div
      bind:this={scrollContainer}
      onscroll={updateScrollState}
      class="flex-1 flex items-center gap-0.5 overflow-x-auto scrollbar-hide mx-1"
    >
      {#each openFiles as file, i}
        {@const isActive = i === activeIndex}
        {@const isFileDirty = file.path === activePath && isDirty}

        <!-- Tab divider (before tab, except first and after active) -->
        {#if i > 0 && i !== activeIndex && i - 1 !== activeIndex}
          <div class="w-px h-4 bg-muted-foreground/30 shrink-0"></div>
        {/if}

        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          onclick={() => handleTabClick(i)}
          class="group flex items-center gap-2 h-8 px-3 min-w-[120px] max-w-[200px] rounded-md text-sm transition-colors shrink-0 cursor-pointer
                 {isActive ? 'bg-background text-foreground font-medium shadow-sm' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'}"
        >
          {#if isFileDirty}
            <span class="w-2 h-2 bg-primary rounded-full shrink-0"></span>
          {/if}
          <span class="truncate flex-1 text-left">{getFileName(file.path)}</span>
          <button
            onclick={(e) => handleClose(e, file.path)}
            class="p-0.5 rounded-full hover:bg-muted shrink-0
                   {isActive ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'}"
            title="Close tab"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M18 6 6 18"/>
              <path d="m6 6 12 12"/>
            </svg>
          </button>
        </div>
      {/each}
    </div>

    <!-- Scroll right button -->
    {#if canScrollRight}
      <button
        onclick={scrollRight}
        class="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground shrink-0"
        title="Scroll right"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="m9 18 6-6-6-6"/>
        </svg>
      </button>
    {/if}

    <!-- Action buttons -->
    <div class="flex items-center gap-1 ml-1 shrink-0">
      <!-- New file/folder dropdown -->
      <div class="relative" bind:this={newMenuRef}>
        <button
          onclick={toggleNewMenu}
          class="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground flex items-center gap-0.5"
          title="New..."
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 5v14"/>
            <path d="M5 12h14"/>
          </svg>
          <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="opacity-50">
            <path d="m6 9 6 6 6-6"/>
          </svg>
        </button>

        {#if showNewMenu}
          <div class="absolute right-0 top-full mt-1 bg-popover border border-border rounded-md shadow-lg py-1 min-w-[140px] z-50">
            <button
              onclick={handleNewFile}
              class="w-full px-3 py-1.5 text-sm text-left hover:bg-accent flex items-center gap-2"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/>
                <path d="M14 2v4a2 2 0 0 0 2 2h4"/>
              </svg>
              New File
            </button>
            <button
              onclick={handleNewFolder}
              class="w-full px-3 py-1.5 text-sm text-left hover:bg-accent flex items-center gap-2"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
              </svg>
              New Folder
            </button>
          </div>
        {/if}
      </div>

      {#if onOpenFolder}
        <button
          onclick={onOpenFolder}
          class="p-1.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground"
          title="Open folder"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
          </svg>
        </button>
      {/if}
    </div>
  </div>
