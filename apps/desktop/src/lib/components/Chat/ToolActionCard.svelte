<script lang="ts">
  import type { ToolAction } from '@midlight/core/types';

  interface Props {
    action: ToolAction;
    onAccept?: () => void;
    onReject?: () => void;
  }

  let { action, onAccept, onReject }: Props = $props();

  // Map action types to icons (using simple text icons for now)
  const iconMap: Record<ToolAction['type'], string> = {
    create: '+',
    edit: '~',
    delete: '-',
    move: '>',
    read: 'R',
    list: 'L',
    search: '?',
    web_search: 'W',
  };

  // Map action types to colors
  const colorMap: Record<ToolAction['type'], string> = {
    create: 'text-green-400',
    edit: 'text-blue-400',
    delete: 'text-red-400',
    move: 'text-yellow-400',
    read: 'text-neutral-400',
    list: 'text-neutral-400',
    search: 'text-purple-400',
    web_search: 'text-cyan-400',
  };

  // Map status to indicator styles
  const statusStyles: Record<ToolAction['status'], { bg: string; animate: boolean }> = {
    pending: { bg: 'bg-neutral-500', animate: false },
    running: { bg: 'bg-blue-500', animate: true },
    complete: { bg: 'bg-green-500', animate: false },
    error: { bg: 'bg-red-500', animate: false },
  };

  const icon = $derived(iconMap[action.type] || '?');
  const colorClass = $derived(colorMap[action.type] || 'text-neutral-400');
  const status = $derived(statusStyles[action.status] || statusStyles.pending);
  const showActions = $derived(action.type === 'edit' && action.status === 'complete' && onAccept && onReject);
</script>

<div class="tool-action-card rounded-lg border border-neutral-700 bg-neutral-800/50 p-3 my-2">
  <div class="flex items-start gap-3">
    <!-- Icon -->
    <div class="flex-shrink-0 w-8 h-8 rounded-md bg-neutral-700/50 flex items-center justify-center font-mono text-sm {colorClass}">
      {icon}
    </div>

    <!-- Content -->
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2">
        <span class="text-sm text-neutral-200">{action.label}</span>

        <!-- Status indicator -->
        <span
          class="w-2 h-2 rounded-full {status.bg}"
          class:animate-pulse={status.animate}
        ></span>
      </div>

      {#if action.path}
        <div class="text-xs text-neutral-500 truncate mt-0.5 font-mono">
          {action.path}
        </div>
      {/if}

      {#if action.status === 'error' && action.result}
        <div class="text-xs text-red-400 mt-1">
          Error: {typeof action.result === 'object' && action.result !== null && 'error' in action.result
            ? (action.result as { error: string }).error
            : 'Unknown error'}
        </div>
      {/if}
    </div>

    <!-- Status badge -->
    <div class="flex-shrink-0">
      {#if action.status === 'running'}
        <span class="text-xs text-blue-400">Running...</span>
      {:else if action.status === 'complete'}
        <span class="text-xs text-green-400">Done</span>
      {:else if action.status === 'error'}
        <span class="text-xs text-red-400">Failed</span>
      {/if}
    </div>
  </div>

  <!-- Accept/Reject buttons for edits -->
  {#if showActions}
    <div class="flex gap-2 mt-3 pt-3 border-t border-neutral-700">
      <button
        class="flex-1 px-3 py-1.5 text-xs rounded-md bg-green-600 hover:bg-green-500 text-white transition-colors"
        onclick={onAccept}
      >
        Accept Changes
      </button>
      <button
        class="flex-1 px-3 py-1.5 text-xs rounded-md bg-neutral-600 hover:bg-neutral-500 text-white transition-colors"
        onclick={onReject}
      >
        Reject
      </button>
    </div>
  {/if}
</div>

<style>
  .tool-action-card {
    backdrop-filter: blur(8px);
  }
</style>
