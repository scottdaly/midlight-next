<script lang="ts">
  import { visibleToasts, hiddenToastCount, toastStore } from '@midlight/stores';
  import Toast from './Toast.svelte';

  let showAll = $state(false);

  function toggleShowAll() {
    showAll = !showAll;
  }

  // Reset showAll when toasts are cleared
  $effect(() => {
    if ($visibleToasts.length === 0) {
      showAll = false;
    }
  });

  const displayToasts = $derived(showAll ? $visibleToasts : $visibleToasts);
</script>

{#if $visibleToasts.length > 0}
  <div
    class="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-sm w-full pointer-events-none"
    aria-live="polite"
    aria-label="Notifications"
  >
    <!-- Hidden count indicator -->
    {#if $hiddenToastCount > 0 && !showAll}
      <button
        class="self-end px-3 py-1.5 text-xs font-medium bg-muted/80 hover:bg-muted text-muted-foreground rounded-full backdrop-blur-sm border border-border shadow-sm transition-colors pointer-events-auto"
        onclick={toggleShowAll}
      >
        +{$hiddenToastCount} more notification{$hiddenToastCount !== 1 ? 's' : ''}
      </button>
    {/if}

    <!-- Toast list -->
    <div class="flex flex-col gap-2 pointer-events-auto">
      {#each displayToasts as toast (toast.id)}
        <Toast {toast} />
      {/each}
    </div>
  </div>
{/if}
