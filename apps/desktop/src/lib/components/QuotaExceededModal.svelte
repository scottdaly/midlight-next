<script lang="ts">
  /**
   * QuotaExceededModal - Shows when user has reached their quota limit
   * Blocks AI features until they upgrade or quota resets
   */

  import { subscription } from '@midlight/stores';

  interface Props {
    open: boolean;
    onClose: () => void;
    onUpgrade: () => void;
  }

  let { open, onClose, onUpgrade }: Props = $props();

  // Get reset date if available
  const resetInfo = $derived(() => {
    // For now, show "next month" - this can be enhanced with actual reset date from backend
    return 'next month';
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function handleBackdropClick() {
    onClose();
  }

  function handleModalClick(e: MouseEvent) {
    e.stopPropagation();
  }

  function handleUpgrade() {
    onUpgrade();
    onClose();
  }
</script>

<svelte:window onkeydown={open ? handleKeyDown : undefined} />

{#if open}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={handleBackdropClick}
  >
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="bg-popover border border-border rounded-lg shadow-xl w-full max-w-md mx-4"
      onclick={handleModalClick}
    >
      <!-- Header with icon -->
      <div class="flex flex-col items-center pt-8 pb-4 px-6">
        <div class="w-16 h-16 rounded-full bg-destructive/10 flex items-center justify-center mb-4">
          <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-destructive">
            <circle cx="12" cy="12" r="10"/>
            <line x1="12" y1="8" x2="12" y2="12"/>
            <line x1="12" y1="16" x2="12.01" y2="16"/>
          </svg>
        </div>
        <h2 class="text-xl font-semibold text-center">Message Limit Reached</h2>
        <p class="text-sm text-muted-foreground text-center mt-2">
          You've used all {$subscription.quota?.limit ?? 100} of your free AI messages this month.
        </p>
      </div>

      <!-- Content -->
      <div class="px-6 pb-4">
        <div class="bg-muted rounded-lg p-4 mb-4">
          <h3 class="font-medium mb-2">Upgrade to continue</h3>
          <ul class="text-sm text-muted-foreground space-y-1.5">
            <li class="flex items-center gap-2">
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
              Unlimited AI messages
            </li>
            <li class="flex items-center gap-2">
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
              Priority support
            </li>
            <li class="flex items-center gap-2">
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
              Access to latest AI models
            </li>
          </ul>
        </div>

        <p class="text-xs text-muted-foreground text-center">
          Your free quota will reset {resetInfo()}.
        </p>
      </div>

      <!-- Actions -->
      <div class="flex flex-col gap-2 p-4 border-t border-border">
        <button
          onclick={handleUpgrade}
          class="w-full py-2.5 px-4 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition-colors"
        >
          Upgrade Now
        </button>
        <button
          onclick={onClose}
          class="w-full py-2 px-4 text-muted-foreground hover:text-foreground transition-colors text-sm"
        >
          Maybe Later
        </button>
      </div>
    </div>
  </div>
{/if}
