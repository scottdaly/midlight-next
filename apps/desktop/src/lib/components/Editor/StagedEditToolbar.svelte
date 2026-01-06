<script lang="ts">
  import { fileSystem, stagedEdit } from '@midlight/stores';

  interface Props {
    onAccept: () => void;
    onReject: () => void;
  }

  let { onAccept, onReject }: Props = $props();

  // Handle keyboard shortcuts
  function handleKeyDown(event: KeyboardEvent) {
    // Cmd/Ctrl+Enter to accept
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault();
      onAccept();
    }
    // Escape to reject
    if (event.key === 'Escape') {
      event.preventDefault();
      onReject();
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if $stagedEdit}
  <div
    class="fixed bottom-6 left-1/2 -translate-x-1/2 z-50
           bg-popover border border-border rounded-lg shadow-lg
           px-4 py-3 flex items-center gap-4"
  >
    <div class="flex items-center gap-2">
      <div class="w-2 h-2 rounded-full bg-amber-500 animate-pulse"></div>
      <span class="text-sm text-muted-foreground">
        AI made changes to this document
      </span>
    </div>

    <div class="h-4 w-px bg-border"></div>

    <div class="flex items-center gap-2">
      <button
        onclick={onReject}
        class="px-3 py-1.5 rounded-md bg-muted hover:bg-muted/80
               text-foreground text-sm font-medium transition-colors
               flex items-center gap-1.5"
      >
        <span>Reject</span>
        <kbd class="text-xs text-muted-foreground bg-background/50 px-1 rounded">Esc</kbd>
      </button>

      <button
        onclick={onAccept}
        class="px-3 py-1.5 rounded-md bg-green-600 hover:bg-green-500
               text-white text-sm font-medium transition-colors
               flex items-center gap-1.5"
      >
        <span>Accept</span>
        <kbd class="text-xs text-green-200 bg-green-700/50 px-1 rounded">⌘↵</kbd>
      </button>
    </div>
  </div>
{/if}
