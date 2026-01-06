<script lang="ts">
  import type { ThinkingStep, ThinkingStepIcon } from '@midlight/core/types';

  interface Props {
    steps: ThinkingStep[];
    isStreaming: boolean;
  }

  let { steps, isStreaming }: Props = $props();

  let isExpanded = $state(false);

  const completedCount = $derived(steps.filter((s) => s.status === 'completed').length);
  const allCompleted = $derived(steps.every((s) => s.status === 'completed'));

  // When streaming, always show expanded
  // When done, show collapsed summary that can be expanded
  const showExpanded = $derived(isStreaming || isExpanded);

  function toggleExpanded() {
    isExpanded = !isExpanded;
  }

  // Get SVG path for each step icon
  function getIconPath(icon: ThinkingStepIcon): string {
    switch (icon) {
      case 'analyze':
        // Sparkle icon
        return 'M12 2L15.09 8.26L22 9.27L17 14.14L18.18 21.02L12 17.77L5.82 21.02L7 14.14L2 9.27L8.91 8.26L12 2Z';
      case 'read':
        // File list icon
        return 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M16 13H8 M16 17H8 M10 9H8';
      case 'search':
        // Search/magnifier icon
        return 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z';
      case 'web_search':
        // Globe icon
        return 'M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10z M2 12h20 M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z';
      case 'create':
        // File plus icon
        return 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M12 18v-6 M9 15h6';
      case 'edit':
        // Pencil icon
        return 'M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7 M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z';
      case 'folder':
        // Folder plus icon
        return 'M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z M12 11v6 M9 14h6';
      case 'delete':
        // Trash icon
        return 'M3 6h18 M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6 M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2';
      case 'move':
        // Move icon
        return 'M5 9l-3 3 3 3 M9 5l3-3 3 3 M15 19l3-3-3-3 M19 9l-3 3 3 3 M2 12h20 M12 2v20';
      case 'thinking':
      default:
        // Loader icon
        return 'M21 12a9 9 0 11-6.219-8.56';
    }
  }
</script>

{#if steps.length > 0}
  <div class="mb-2">
    <!-- Collapsed summary when done -->
    {#if allCompleted && !isStreaming}
      <button
        onclick={toggleExpanded}
        class="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
      >
        <!-- Checkmark -->
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500">
          <polyline points="20 6 9 17 4 12"/>
        </svg>
        <span>{completedCount} step{completedCount !== 1 ? 's' : ''} completed</span>
        <!-- Expand/collapse arrow -->
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          {#if isExpanded}
            <polyline points="18 15 12 9 6 15"/>
          {:else}
            <polyline points="6 9 12 15 18 9"/>
          {/if}
        </svg>
      </button>
    {/if}

    <!-- Expanded steps list -->
    {#if showExpanded}
      <div class="space-y-0.5 mt-1">
        {#each steps as step (step.id)}
          {@const isActive = step.status === 'active'}
          <div class="flex items-center gap-2 py-0.5">
            <!-- Status indicator -->
            {#if isActive}
              <!-- Spinner -->
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground animate-spin flex-shrink-0">
                <path d="M21 12a9 9 0 11-6.219-8.56"/>
              </svg>
            {:else}
              <!-- Checkmark -->
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500 flex-shrink-0">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
            {/if}

            <!-- Step icon -->
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground flex-shrink-0">
              <path d={getIconPath(step.icon)}/>
            </svg>

            <!-- Label -->
            <span class="text-xs {isActive ? 'text-foreground animate-pulse' : 'text-muted-foreground'}">
              {step.label}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
