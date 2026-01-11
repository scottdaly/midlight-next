<script lang="ts">
  import {
    contextUpdateStore,
    pendingContextUpdate,
    showContextUpdateDialog,
    isApplyingContextUpdate,
  } from '@midlight/stores';

  function handleConfirm() {
    contextUpdateStore.confirmPendingUpdate();
  }

  function handleReject() {
    contextUpdateStore.rejectPendingUpdate();
  }

  function handleBackdropClick() {
    contextUpdateStore.closeConfirmDialog();
  }

  function handleModalClick(e: MouseEvent) {
    e.stopPropagation();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      contextUpdateStore.closeConfirmDialog();
    }
  }
</script>

<svelte:window onkeydown={$showContextUpdateDialog ? handleKeyDown : undefined} />

{#if $showContextUpdateDialog && $pendingContextUpdate}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4"
    onclick={handleBackdropClick}
  >
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="bg-card border border-border rounded-lg shadow-xl max-w-lg w-full max-h-[80vh] flex flex-col"
      onclick={handleModalClick}
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-border">
        <div class="flex items-center gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary">
            <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
            <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
          </svg>
          <h2 class="text-sm font-semibold">Update Project Context?</h2>
        </div>
        <button
          onclick={handleReject}
          class="p-1 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors"
          aria-label="Close"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6 6 18"/>
            <path d="M6 6 18 18"/>
          </svg>
        </button>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto p-4 space-y-4">
        <p class="text-sm text-muted-foreground">
          Based on your conversation, I'd like to update your project context:
        </p>

        <!-- Changes Preview -->
        <div class="space-y-3">
          {#each $pendingContextUpdate.diffs as diff}
            <div class="rounded-lg border border-border overflow-hidden">
              <div class="px-3 py-2 bg-muted/30 text-xs font-medium border-b border-border">
                {diff.section}
              </div>
              <div class="p-3 space-y-2">
                {#if diff.oldValue}
                  <div class="text-xs">
                    <span class="text-muted-foreground">Before:</span>
                    <div class="mt-1 p-2 bg-destructive/10 rounded text-sm line-through opacity-70">
                      {diff.oldValue}
                    </div>
                  </div>
                {/if}
                {#if diff.newValue}
                  <div class="text-xs">
                    <span class="text-muted-foreground">{diff.oldValue ? 'After:' : 'Adding:'}</span>
                    <div class="mt-1 p-2 bg-green-500/10 rounded text-sm">
                      {diff.newValue}
                    </div>
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>

        <!-- Update Summary -->
        <div class="text-xs text-muted-foreground">
          <p class="font-medium mb-1">Updates:</p>
          <ul class="list-disc list-inside space-y-0.5">
            {#each $pendingContextUpdate.updates as update}
              <li>{update.reason}</li>
            {/each}
          </ul>
        </div>
      </div>

      <!-- Footer -->
      <div class="flex items-center justify-end gap-2 px-4 py-3 border-t border-border bg-muted/30">
        <button
          onclick={handleReject}
          disabled={$isApplyingContextUpdate}
          class="px-3 py-1.5 text-sm border border-border rounded-md hover:bg-muted transition-colors disabled:opacity-50"
        >
          Skip
        </button>
        <button
          onclick={handleConfirm}
          disabled={$isApplyingContextUpdate}
          class="px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors disabled:opacity-50 flex items-center gap-2"
        >
          {#if $isApplyingContextUpdate}
            <svg class="animate-spin h-3 w-3" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            Updating...
          {:else}
            Update Context
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}
