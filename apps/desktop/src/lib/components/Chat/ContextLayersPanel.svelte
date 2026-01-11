<script lang="ts">
  import { contextLayers, totalContextTokens, freshStartMode } from '@midlight/stores';
  import type { ContextLayerType } from '@midlight/stores';

  interface Props {
    expanded?: boolean;
  }

  let { expanded = false }: Props = $props();

  let isExpanded = $state(expanded);

  // Icon and color mapping for layer types
  function getLayerInfo(type: ContextLayerType): { icon: string; label: string; color: string } {
    switch (type) {
      case 'global':
        return { icon: 'user', label: 'About Me', color: 'text-blue-500' };
      case 'project':
        return { icon: 'folder', label: 'Project Context', color: 'text-purple-500' };
      case 'document':
        return { icon: 'file', label: 'Current Document', color: 'text-green-500' };
      case 'mentioned':
        return { icon: 'at', label: 'Referenced', color: 'text-orange-500' };
      case 'selection':
        return { icon: 'highlight', label: 'Selection', color: 'text-amber-500' };
      default:
        return { icon: 'file', label: 'Unknown', color: 'text-muted-foreground' };
    }
  }

  // Format token count
  function formatTokens(count: number): string {
    if (count < 1000) return `~${count}`;
    return `~${(count / 1000).toFixed(1)}k`;
  }
</script>

{#if $contextLayers.length > 0}
  <div class="border-t border-border/50">
    <!-- Header -->
    <button
      type="button"
      onclick={() => isExpanded = !isExpanded}
      class="w-full flex items-center justify-between px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
    >
      <div class="flex items-center gap-2">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="transition-transform {isExpanded ? 'rotate-90' : ''}"
        >
          <polyline points="9 18 15 12 9 6"/>
        </svg>
        <span>Context ({$contextLayers.length} layers)</span>
        {#if $freshStartMode}
          <span class="px-1.5 py-0.5 rounded text-[10px] bg-amber-500/20 text-amber-600 dark:text-amber-400">
            Fresh Start
          </span>
        {/if}
      </div>
      <span class="text-[10px] opacity-70">{formatTokens($totalContextTokens)} tokens</span>
    </button>

    <!-- Expanded content -->
    {#if isExpanded}
      <div class="px-3 pb-2 space-y-1">
        {#each $contextLayers as layer}
          {@const info = getLayerInfo(layer.type)}
          <div class="flex items-center justify-between text-xs py-1 px-2 rounded bg-muted/30">
            <div class="flex items-center gap-2 min-w-0">
              <!-- Layer type icon -->
              {#if info.icon === 'user'}
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={info.color}>
                  <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/>
                  <circle cx="12" cy="7" r="4"/>
                </svg>
              {:else if info.icon === 'folder'}
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={info.color}>
                  <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                </svg>
              {:else if info.icon === 'file'}
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={info.color}>
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                  <polyline points="14 2 14 8 20 8"/>
                </svg>
              {:else if info.icon === 'at'}
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={info.color}>
                  <circle cx="12" cy="12" r="4"/>
                  <path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.92 7.94"/>
                </svg>
              {:else if info.icon === 'highlight'}
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={info.color}>
                  <path d="m9 11-6 6v3h9l3-3"/>
                  <path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"/>
                </svg>
              {/if}

              <span class="truncate" title={layer.source}>
                {layer.source}
              </span>
            </div>
            <span class="text-[10px] text-muted-foreground flex-shrink-0 ml-2">
              {formatTokens(layer.tokenEstimate || 0)}
            </span>
          </div>
        {/each}

        {#if $freshStartMode}
          <p class="text-[10px] text-muted-foreground italic mt-1 px-2">
            Fresh Start mode is active. Project and global context are disabled.
          </p>
        {/if}
      </div>
    {/if}
  </div>
{/if}
