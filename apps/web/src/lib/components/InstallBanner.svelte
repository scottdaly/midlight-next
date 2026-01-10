<script lang="ts">
  import { pwa, showInstallBanner } from '@midlight/stores';
  import { onMount } from 'svelte';
  import { slide } from 'svelte/transition';

  let isVisible = $state(false);
  let isInstalling = $state(false);

  onMount(() => {
    pwa.init();
  });

  $effect(() => {
    isVisible = $showInstallBanner;
  });

  async function handleInstall() {
    isInstalling = true;
    await pwa.promptInstall();
    isInstalling = false;
  }

  function handleDismiss() {
    pwa.resetDismissed();
    // The store will update wasDismissed, hiding the banner
    isVisible = false;
  }
</script>

{#if isVisible}
  <div
    class="fixed bottom-4 right-4 z-50 max-w-sm"
    transition:slide={{ duration: 300 }}
  >
    <div class="bg-card border border-border rounded-lg shadow-lg p-4">
      <div class="flex items-start gap-3">
        <!-- Icon -->
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center">
          <svg
            class="w-6 h-6 text-primary"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
            />
          </svg>
        </div>

        <!-- Content -->
        <div class="flex-1 min-w-0">
          <h3 class="text-sm font-semibold text-foreground">
            Install Midlight
          </h3>
          <p class="text-xs text-muted-foreground mt-1">
            {#if $pwa.platform === 'ios'}
              Add to your home screen for quick access and offline support.
            {:else}
              Install for a faster experience and offline access.
            {/if}
          </p>

          <!-- Actions -->
          <div class="flex items-center gap-2 mt-3">
            {#if $pwa.platform === 'ios'}
              <!-- iOS needs manual install -->
              <p class="text-xs text-muted-foreground">
                Tap <span class="inline-flex items-center">
                  <svg class="w-4 h-4 mx-0.5" fill="currentColor" viewBox="0 0 20 20">
                    <path d="M15 8a1 1 0 01-1 1h-1v1a1 1 0 11-2 0V9H9a1 1 0 010-2h2V6a1 1 0 112 0v1h1a1 1 0 011 1z"/>
                    <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h8a2 2 0 012 2v12a2 2 0 01-2 2H6a2 2 0 01-2-2V4zm2 0v12h8V4H6z" clip-rule="evenodd"/>
                  </svg>
                </span> Share, then "Add to Home Screen"
              </p>
            {:else}
              <button
                class="px-3 py-1.5 text-xs font-medium bg-primary text-primary-foreground rounded hover:bg-primary/90 transition-colors disabled:opacity-50"
                onclick={handleInstall}
                disabled={isInstalling}
              >
                {#if isInstalling}
                  Installing...
                {:else}
                  Install
                {/if}
              </button>
            {/if}

            <button
              class="px-3 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
              onclick={handleDismiss}
            >
              Not now
            </button>
          </div>
        </div>

        <!-- Close button -->
        <button
          class="flex-shrink-0 p-1 text-muted-foreground hover:text-foreground transition-colors"
          onclick={handleDismiss}
          aria-label="Dismiss"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      </div>
    </div>
  </div>
{/if}
