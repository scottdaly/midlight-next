<script lang="ts">
  interface DiffResult {
    additions: string[];
    deletions: string[];
    changeCount: number;
  }

  interface Props {
    diff: DiffResult | null;
    baseLabel: string;
    compareLabel: string;
    isLoading?: boolean;
    onClose: () => void;
  }

  let { diff, baseLabel, compareLabel, isLoading = false, onClose }: Props = $props();

  type ViewMode = 'split' | 'unified';
  let viewMode = $state<ViewMode>('unified');
</script>

<div class="h-full flex flex-col bg-background">
  <!-- Header -->
  <div class="h-10 border-b border-border flex items-center justify-between px-3 bg-card">
    <div class="flex items-center gap-2">
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
        <path d="M12 3v14"/>
        <path d="M5 10h14"/>
        <path d="M5 21h14"/>
      </svg>
      <span class="text-sm font-medium">Compare Versions</span>
    </div>
    <div class="flex items-center gap-2">
      <!-- View mode toggle -->
      <div class="flex rounded-md border border-border overflow-hidden">
        <button
          onclick={() => viewMode = 'unified'}
          class="px-2 py-1 text-xs {viewMode === 'unified' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'}"
        >
          Unified
        </button>
        <button
          onclick={() => viewMode = 'split'}
          class="px-2 py-1 text-xs {viewMode === 'split' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'}"
        >
          Split
        </button>
      </div>
      <button
        onclick={onClose}
        class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground"
        title="Close comparison"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M18 6 6 18"/>
          <path d="m6 6 12 12"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- Labels -->
  <div class="flex border-b border-border text-xs">
    <div class="flex-1 px-3 py-2 bg-red-500/5 text-red-600 border-r border-border">
      <span class="font-medium">Base:</span> {baseLabel}
    </div>
    <div class="flex-1 px-3 py-2 bg-green-500/5 text-green-600">
      <span class="font-medium">Compare:</span> {compareLabel}
    </div>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-auto">
    {#if isLoading}
      <div class="flex items-center justify-center py-8">
        <div class="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
      </div>
    {:else if !diff}
      <div class="text-center text-muted-foreground text-sm py-8">
        <p>No diff data available</p>
      </div>
    {:else if diff.additions.length === 0 && diff.deletions.length === 0}
      <div class="text-center text-muted-foreground text-sm py-8">
        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="mx-auto mb-3 opacity-50">
          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
          <polyline points="22 4 12 14.01 9 11.01"/>
        </svg>
        <p>No differences found</p>
        <p class="text-xs mt-1">These versions are identical</p>
      </div>
    {:else if viewMode === 'unified'}
      <!-- Unified view -->
      <div class="p-3 space-y-1 font-mono text-xs">
        <!-- Summary -->
        <div class="flex gap-4 mb-4 pb-3 border-b border-border text-sm">
          <span class="text-green-600">+{diff.additions.length} additions</span>
          <span class="text-red-600">-{diff.deletions.length} deletions</span>
          <span class="text-muted-foreground">{diff.changeCount} chars changed</span>
        </div>

        <!-- Deletions -->
        {#if diff.deletions.length > 0}
          <div class="mb-4">
            <div class="text-xs text-red-600 font-medium mb-2 font-sans">Removed:</div>
            {#each diff.deletions as line, i}
              <div class="flex">
                <span class="w-8 text-right pr-2 text-muted-foreground select-none">{i + 1}</span>
                <span class="flex-1 px-2 py-0.5 bg-red-500/10 text-red-700 dark:text-red-400 rounded">
                  <span class="select-none text-red-500 mr-1">-</span>{line || '\u00A0'}
                </span>
              </div>
            {/each}
          </div>
        {/if}

        <!-- Additions -->
        {#if diff.additions.length > 0}
          <div>
            <div class="text-xs text-green-600 font-medium mb-2 font-sans">Added:</div>
            {#each diff.additions as line, i}
              <div class="flex">
                <span class="w-8 text-right pr-2 text-muted-foreground select-none">{i + 1}</span>
                <span class="flex-1 px-2 py-0.5 bg-green-500/10 text-green-700 dark:text-green-400 rounded">
                  <span class="select-none text-green-500 mr-1">+</span>{line || '\u00A0'}
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {:else}
      <!-- Split view -->
      <div class="flex h-full">
        <!-- Left side (deletions / base) -->
        <div class="flex-1 border-r border-border overflow-auto">
          <div class="p-3 font-mono text-xs">
            {#if diff.deletions.length > 0}
              {#each diff.deletions as line, i}
                <div class="flex">
                  <span class="w-8 text-right pr-2 text-muted-foreground select-none">{i + 1}</span>
                  <span class="flex-1 px-2 py-0.5 bg-red-500/10 text-red-700 dark:text-red-400">
                    {line || '\u00A0'}
                  </span>
                </div>
              {/each}
            {:else}
              <div class="text-muted-foreground text-center py-4 font-sans">
                No deletions
              </div>
            {/if}
          </div>
        </div>

        <!-- Right side (additions / compare) -->
        <div class="flex-1 overflow-auto">
          <div class="p-3 font-mono text-xs">
            {#if diff.additions.length > 0}
              {#each diff.additions as line, i}
                <div class="flex">
                  <span class="w-8 text-right pr-2 text-muted-foreground select-none">{i + 1}</span>
                  <span class="flex-1 px-2 py-0.5 bg-green-500/10 text-green-700 dark:text-green-400">
                    {line || '\u00A0'}
                  </span>
                </div>
              {/each}
            {:else}
              <div class="text-muted-foreground text-center py-4 font-sans">
                No additions
              </div>
            {/if}
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>
