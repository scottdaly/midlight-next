<script lang="ts">
  import SearchDropdown from './SearchDropdown.svelte';

  let isOpen = $state(false);
  let query = $state('');
  let containerRef: HTMLDivElement | null = $state(null);
  let inputRef: HTMLInputElement | null = $state(null);

  // Platform detection
  const isMac = navigator.userAgent.includes('Mac');

  function handleFocus() {
    isOpen = true;
  }

  function handleClose() {
    isOpen = false;
    query = '';
    inputRef?.blur();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      handleClose();
    }
  }

  // Global shortcut
  $effect(() => {
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        isOpen = true;
        // Focus next tick
        setTimeout(() => inputRef?.focus(), 10);
      }
    };

    window.addEventListener('keydown', handleGlobalKeyDown);
    return () => window.removeEventListener('keydown', handleGlobalKeyDown);
  });

  // Click outside
  $effect(() => {
    if (!isOpen) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef && !containerRef.contains(e.target as Node)) {
        isOpen = false;
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  });
</script>

<div bind:this={containerRef} class="relative">
  <!-- Search Input Container -->
  <!-- data-tauri-drag-region should be avoided on interactive elements, but this container is not interactive itself?
       Actually, the input needs to be interactive.
       The parent TitleBar has data-tauri-drag-region.
       We should probably prevent drag on this component.
  -->
  <div class="flex items-center gap-2 w-80 px-3 py-1 rounded-lg border transition-colors
    {isOpen ? 'bg-background border-foreground/30 shadow-md' : 'bg-muted/50 hover:bg-muted/70 border-foreground/20'}"
  >
    <!-- Search Icon -->
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-muted-foreground shrink-0"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>

    <input
      bind:this={inputRef}
      type="text"
      bind:value={query}
      onfocus={handleFocus}
      onkeydown={handleKeyDown}
      placeholder="Search..."
      class="flex-1 bg-transparent outline-none text-sm text-foreground placeholder:text-muted-foreground min-w-0"
      spellcheck="false"
      autocomplete="off"
    />

    <kbd class="text-xs text-muted-foreground/70 bg-muted/50 px-1.5 py-0.5 rounded shrink-0 font-mono">
      {isMac ? '⌘K' : 'Ctrl+K'}
    </kbd>
  </div>

  <SearchDropdown
    open={isOpen}
    query={query}
    onClose={handleClose}
    inputElement={inputRef}
  />
</div>
