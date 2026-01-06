<script lang="ts">
  import type { ToolAction } from '@midlight/core/types';
  import ToolActionCard from './ToolActionCard.svelte';
  import { agent } from '@midlight/stores';

  interface Props {
    actions: ToolAction[];
    onRefreshFiles?: () => void;
  }

  let { actions, onRefreshFiles }: Props = $props();

  const completedCount = $derived(actions.filter((a) => a.status === 'complete').length);
  const totalCount = $derived(actions.length);
  const isAllComplete = $derived(completedCount === totalCount && totalCount > 0);

  function handleAccept(action: ToolAction) {
    // Accept the pending change in the agent store
    if (action.result && typeof action.result === 'object' && 'changeId' in action.result) {
      agent.acceptChange((action.result as { changeId: string }).changeId);
      // Refresh the file tree if callback provided
      onRefreshFiles?.();
    }
  }

  function handleReject(action: ToolAction) {
    // Reject the pending change in the agent store
    if (action.result && typeof action.result === 'object' && 'changeId' in action.result) {
      agent.rejectChange((action.result as { changeId: string }).changeId);
      // TODO: Actually revert the file content
    }
  }
</script>

{#if actions.length > 0}
  <div class="tool-actions-group">
    <!-- Summary header -->
    <div class="flex items-center gap-2 mb-2 text-xs text-neutral-500">
      <span class="font-medium">
        {#if isAllComplete}
          Completed {totalCount} action{totalCount > 1 ? 's' : ''}
        {:else}
          Running actions ({completedCount}/{totalCount})
        {/if}
      </span>
    </div>

    <!-- Action cards -->
    <div class="space-y-2">
      {#each actions as action (action.id)}
        <ToolActionCard
          {action}
          onAccept={action.type === 'edit' ? () => handleAccept(action) : undefined}
          onReject={action.type === 'edit' ? () => handleReject(action) : undefined}
        />
      {/each}
    </div>
  </div>
{/if}
