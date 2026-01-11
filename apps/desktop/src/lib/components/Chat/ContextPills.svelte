<script lang="ts">
  import { ai } from '@midlight/stores';
  import type { ContextItem } from '@midlight/stores/ai';

  // Get display name for context item
  function getDisplayName(item: ContextItem): string {
    // Label already includes project prefix if from another project
    if (item.type === 'file' && item.label.endsWith('.midlight')) {
      return item.label.slice(0, -9);
    }
    return item.label;
  }

  // Check if item is from another project
  function isCrossProject(item: ContextItem): boolean {
    return item.projectPath !== undefined && item.projectName !== undefined;
  }

  // Get tooltip with full path
  function getTooltip(item: ContextItem): string {
    if (item.path) {
      if (item.projectName) {
        return `${item.projectName}: ${item.path}`;
      }
      return item.path;
    }
    return item.label;
  }

  function removeItem(index: number) {
    ai.removeContextItem(index);
  }
</script>

{#if $ai.contextItems.length > 0}
  <div class="flex flex-wrap gap-1.5 px-3 py-2 border-b border-border">
    {#each $ai.contextItems as item, i}
      <div
        class="flex items-center gap-1 rounded-full pl-2 pr-1 py-0.5 text-xs {isCrossProject(item) ? 'bg-primary/20 border border-primary/30' : 'bg-accent'}"
        title={getTooltip(item)}
      >
        {#if isCrossProject(item)}
          <!-- Cross-project file icon (briefcase + file) -->
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary">
            <rect width="20" height="14" x="2" y="7" rx="2" ry="2"/>
            <path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16"/>
          </svg>
        {:else if item.type === 'file'}
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <path d="M14 2v6h6"/>
          </svg>
        {:else if item.type === 'selection'}
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
            <path d="M3 6h18"/>
            <path d="M3 12h18"/>
            <path d="M3 18h18"/>
          </svg>
        {:else}
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <path d="M14 2v6h6"/>
            <line x1="16" y1="13" x2="8" y2="13"/>
            <line x1="16" y1="17" x2="8" y2="17"/>
          </svg>
        {/if}
        <span class="max-w-40 truncate">{getDisplayName(item)}</span>
        <button
          onclick={() => removeItem(i)}
          class="p-0.5 hover:bg-background rounded-full"
          title="Remove"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
    {/each}
  </div>
{/if}
