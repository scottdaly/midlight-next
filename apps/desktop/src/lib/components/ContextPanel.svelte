<script lang="ts">
  import { ai, contextLayers, freshStartMode } from '@midlight/stores';
  import type { ContextLayer, ContextLayerType } from '@midlight/stores';

  // Token budget configuration (can be adjusted per model)
  const TOKEN_BUDGET = 32000;

  // Track collapsed state per layer type
  let collapsedLayers = $state<Set<ContextLayerType>>(new Set());

  // Track disabled layers (for toggle functionality)
  let disabledLayers = $state<Set<ContextLayerType>>(new Set());

  // Group layers by type
  const groupedLayers = $derived(() => {
    const groups: Record<ContextLayerType, ContextLayer[]> = {
      global: [],
      project: [],
      document: [],
      mentioned: [],
      selection: [],
      semantic: [],
    };

    for (const layer of $contextLayers) {
      groups[layer.type].push(layer);
    }

    return groups;
  });

  // Calculate total tokens (excluding disabled layers)
  const totalTokens = $derived(
    $contextLayers
      .filter(layer => !disabledLayers.has(layer.type))
      .reduce((sum, layer) => sum + (layer.tokenEstimate || 0), 0)
  );

  // Token budget percentage
  const budgetPercentage = $derived(Math.min((totalTokens / TOKEN_BUDGET) * 100, 100));

  // Budget color based on usage
  const budgetColor = $derived(() => {
    if (budgetPercentage < 50) return 'bg-green-500';
    if (budgetPercentage < 80) return 'bg-yellow-500';
    return 'bg-red-500';
  });

  // Layer type metadata for display
  const layerMeta: Record<ContextLayerType, { icon: string; label: string; color: string }> = {
    global: { icon: '🌍', label: 'Global Context', color: 'text-blue-500' },
    project: { icon: '📁', label: 'Project Context', color: 'text-purple-500' },
    document: { icon: '📄', label: 'Current Document', color: 'text-green-500' },
    mentioned: { icon: '@', label: '@-Mentioned Files', color: 'text-orange-500' },
    selection: { icon: '✂️', label: 'Selected Text', color: 'text-yellow-500' },
    semantic: { icon: '🔍', label: 'Semantic Search', color: 'text-cyan-500' },
  };

  // Layer order for rendering
  const layerOrder: ContextLayerType[] = ['global', 'project', 'document', 'mentioned', 'selection', 'semantic'];

  function toggleCollapsed(type: ContextLayerType) {
    if (collapsedLayers.has(type)) {
      collapsedLayers.delete(type);
    } else {
      collapsedLayers.add(type);
    }
    collapsedLayers = new Set(collapsedLayers);
  }

  function toggleLayerEnabled(type: ContextLayerType) {
    if (disabledLayers.has(type)) {
      disabledLayers.delete(type);
    } else {
      disabledLayers.add(type);
    }
    disabledLayers = new Set(disabledLayers);
  }

  function clearAllMentions() {
    ai.setContextItems([]);
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

  // Check if there are any mentioned items
  const hasMentions = $derived(groupedLayers().mentioned.length > 0);
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

  <!-- Clear Mentions Button -->
  {#if hasMentions}
    <div class="px-3 py-2 border-b border-border">
      <button
        onclick={clearAllMentions}
        class="w-full px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-accent rounded transition-colors flex items-center justify-center gap-1.5"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M18 6 6 18"/>
          <path d="m6 6 12 12"/>
        </svg>
        Clear all @-mentions
      </button>
    </div>
  {/if}

  <!-- Context Layers List -->
  <div class="flex-1 overflow-auto p-3 space-y-3">
    {#each layerOrder as type}
      {@const layers = groupedLayers()[type]}
      {@const meta = layerMeta[type]}
      {@const isCollapsed = collapsedLayers.has(type)}
      {@const isDisabled = disabledLayers.has(type)}
      {@const layerTokens = layers.reduce((sum, l) => sum + (l.tokenEstimate || 0), 0)}

      {#if layers.length > 0}
        <div class="rounded-lg border border-border bg-muted/30 {isDisabled ? 'opacity-50' : ''}">
          <!-- Layer Header -->
          <div class="flex items-center px-3 py-2 text-sm">
            <!-- Toggle Switch -->
            <button
              onclick={(e) => { e.stopPropagation(); toggleLayerEnabled(type); }}
              class="mr-2 relative w-8 h-4 rounded-full transition-colors {isDisabled ? 'bg-muted' : 'bg-primary'}"
              title={isDisabled ? 'Enable layer' : 'Disable layer'}
            >
              <span class="absolute top-0.5 left-0.5 w-3 h-3 rounded-full bg-white transition-transform {isDisabled ? '' : 'translate-x-4'}"></span>
            </button>

            <!-- Expand/Collapse Button -->
            <button
              onclick={() => toggleCollapsed(type)}
              class="flex-1 flex items-center justify-between hover:bg-accent/50 transition-colors rounded -ml-1 pl-1 py-0.5"
            >
              <div class="flex items-center gap-2">
                <span class={meta.color}>{meta.icon}</span>
                <span class="font-medium text-foreground">{meta.label}</span>
                {#if layers.length > 1}
                  <span class="text-xs text-muted-foreground">({layers.length})</span>
                {/if}
              </div>
              <div class="flex items-center gap-2">
                <span class="text-xs text-muted-foreground {isDisabled ? 'line-through' : ''}">~{formatTokens(layerTokens)} tk</span>
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
          </div>

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

  <!-- Footer with Token Budget -->
  <div class="border-t border-border px-3 py-2 space-y-2">
    <!-- Token Budget Progress Bar -->
    <div class="space-y-1">
      <div class="flex items-center justify-between text-xs">
        <span class="text-muted-foreground">Token Budget</span>
        <span class="text-foreground font-medium">
          {formatTokens(totalTokens)} / {formatTokens(TOKEN_BUDGET)}
          <span class="text-muted-foreground ml-1">({budgetPercentage.toFixed(0)}%)</span>
        </span>
      </div>
      <div class="h-2 bg-muted rounded-full overflow-hidden">
        <div
          class="h-full transition-all duration-300 {budgetColor()}"
          style="width: {budgetPercentage}%"
        ></div>
      </div>
      {#if budgetPercentage >= 80}
        <p class="text-[10px] text-yellow-600">
          {#if budgetPercentage >= 100}
            Context limit reached. Consider disabling some layers.
          {:else}
            Approaching context limit. Some content may be truncated.
          {/if}
        </p>
      {/if}
    </div>
  </div>
</div>
