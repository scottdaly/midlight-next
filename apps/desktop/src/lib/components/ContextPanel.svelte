<script lang="ts">
  import { ai, contextLayers, freshStartMode } from '@midlight/stores';
  import type { ContextLayer, ContextLayerType } from '@midlight/stores';

  // Track collapsed state per layer type
  let collapsedLayers = $state<Set<ContextLayerType>>(new Set());

  // Group layers by type
  const groupedLayers = $derived(() => {
    const groups: Record<ContextLayerType, ContextLayer[]> = {
      global: [],
      project: [],
      document: [],
      mentioned: [],
      selection: [],
    };

    for (const layer of $contextLayers) {
      groups[layer.type].push(layer);
    }

    return groups;
  });

  // Calculate total tokens
  const totalTokens = $derived(
    $contextLayers.reduce((sum, layer) => sum + (layer.tokenEstimate || 0), 0)
  );

  // Layer type metadata for display
  const layerMeta: Record<ContextLayerType, { icon: string; label: string; color: string }> = {
    global: { icon: '🌍', label: 'Global Context', color: 'text-blue-500' },
    project: { icon: '📁', label: 'Project Context', color: 'text-purple-500' },
    document: { icon: '📄', label: 'Current Document', color: 'text-green-500' },
    mentioned: { icon: '@', label: '@-Mentioned Files', color: 'text-orange-500' },
    selection: { icon: '✂️', label: 'Selected Text', color: 'text-yellow-500' },
  };

  function toggleCollapsed(type: ContextLayerType) {
    if (collapsedLayers.has(type)) {
      collapsedLayers.delete(type);
    } else {
      collapsedLayers.add(type);
    }
    collapsedLayers = new Set(collapsedLayers);
  }

  function formatTokens(tokens: number): string {
    if (tokens >= 1000) {
      return `${(tokens / 1000).toFixed(1)}k`;
    }
    return tokens.toString();
  }

  function getPreview(content: string, maxLength: number = 200): string {
    if (content.length <= maxLength) return content;
    return content.slice(0, maxLength).trim() + '...';
  }

  function toggleFreshStart() {
    ai.toggleFreshStartMode();
  }
</script>

<div class="h-full flex flex-col bg-card">
  <!-- Header -->
  <div class="h-10 border-b border-border flex items-center justify-between px-3">
    <span class="text-sm font-medium text-foreground">Context Layers</span>
    <button
      onclick={toggleFreshStart}
      class="px-2 py-1 text-xs rounded transition-colors {$freshStartMode
        ? 'bg-yellow-500/20 text-yellow-600 hover:bg-yellow-500/30'
        : 'bg-muted text-muted-foreground hover:bg-accent hover:text-foreground'}"
      title={$freshStartMode ? 'Fresh Start mode active - only @-mentions included' : 'Enable Fresh Start mode'}
    >
      {$freshStartMode ? '🌱 Fresh Start' : 'Normal'}
    </button>
  </div>

  <!-- Fresh Start Banner -->
  {#if $freshStartMode}
    <div class="px-3 py-2 bg-yellow-500/10 border-b border-yellow-500/20 text-xs text-yellow-600">
      <strong>Fresh Start Mode:</strong> Only @-mentioned files are included in context. Persistent context is disabled.
    </div>
  {/if}

  <!-- Context Layers List -->
  <div class="flex-1 overflow-auto p-3 space-y-3">
    {#each (['global', 'project', 'document', 'mentioned', 'selection'] as ContextLayerType[]) as type}
      {@const layers = groupedLayers()[type]}
      {@const meta = layerMeta[type]}
      {@const isCollapsed = collapsedLayers.has(type)}
      {@const layerTokens = layers.reduce((sum, l) => sum + (l.tokenEstimate || 0), 0)}

      {#if layers.length > 0}
        <div class="rounded-lg border border-border bg-muted/30">
          <!-- Layer Header -->
          <button
            onclick={() => toggleCollapsed(type)}
            class="w-full flex items-center justify-between px-3 py-2 text-sm hover:bg-accent/50 transition-colors rounded-t-lg"
          >
            <div class="flex items-center gap-2">
              <span class={meta.color}>{meta.icon}</span>
              <span class="font-medium text-foreground">{meta.label}</span>
              {#if layers.length > 1}
                <span class="text-xs text-muted-foreground">({layers.length})</span>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              <span class="text-xs text-muted-foreground">~{formatTokens(layerTokens)} tk</span>
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                class="text-muted-foreground transition-transform {isCollapsed ? '' : 'rotate-90'}"
              >
                <polyline points="9 18 15 12 9 6"/>
              </svg>
            </div>
          </button>

          <!-- Layer Content -->
          {#if !isCollapsed}
            <div class="border-t border-border px-3 py-2 space-y-2">
              {#each layers as layer}
                <div class="text-xs">
                  <div class="flex items-center justify-between mb-1">
                    <span class="text-muted-foreground font-mono truncate">
                      {layer.source}
                    </span>
                    {#if layer.tokenEstimate}
                      <span class="text-muted-foreground flex-shrink-0 ml-2">
                        ~{formatTokens(layer.tokenEstimate)} tk
                      </span>
                    {/if}
                  </div>
                  <div class="text-foreground/70 bg-background/50 rounded p-2 font-mono text-[11px] whitespace-pre-wrap break-words max-h-32 overflow-auto">
                    {getPreview(layer.content, 300)}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/each}

    {#if $contextLayers.length === 0}
      <div class="text-center py-8 text-muted-foreground text-sm">
        <p>No context layers active</p>
        <p class="text-xs mt-1">Open a document to see context here</p>
      </div>
    {/if}
  </div>

  <!-- Footer with Total -->
  <div class="border-t border-border px-3 py-2 flex items-center justify-between">
    <span class="text-xs text-muted-foreground">Total Context</span>
    <span class="text-sm font-medium text-foreground">~{formatTokens(totalTokens)} tokens</span>
  </div>
</div>
