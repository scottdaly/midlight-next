<script lang="ts">
  import { network, isOffline, hasPendingSyncs } from '@midlight/stores';
  import { onMount } from 'svelte';
  import { fade, slide } from 'svelte/transition';

  let isExpanded = $state(false);
  let showBanner = $state(false);

  // Initialize network monitoring
  onMount(() => {
    network.init();
    return () => network.destroy();
  });

  // Show banner when offline or has pending syncs
  $effect(() => {
    showBanner = $isOffline || $hasPendingSyncs;
  });

  function toggleExpand() {
    isExpanded = !isExpanded;
  }

  function formatTime(date: Date | null): string {
    if (!date) return 'Never';
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const minutes = Math.floor(diff / 60000);

    if (minutes < 1) return 'Just now';
    if (minutes < 60) return `${minutes}m ago`;

    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;

    return date.toLocaleDateString();
  }
</script>

{#if showBanner}
  <div
    class="fixed bottom-4 left-4 z-50"
    transition:fade={{ duration: 200 }}
  >
    <div
      class="bg-card border border-border rounded-lg shadow-lg overflow-hidden max-w-xs"
    >
      <!-- Main indicator bar -->
      <button
        class="w-full flex items-center gap-3 px-4 py-3 text-left hover:bg-accent/50 transition-colors"
        onclick={toggleExpand}
      >
        <!-- Status icon -->
        {#if $isOffline}
          <div class="flex-shrink-0">
            <svg
              class="w-5 h-5 text-amber-500"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M18.364 5.636a9 9 0 010 12.728m0 0l-2.829-2.829m2.829 2.829L21 21M15.536 8.464a5 5 0 010 7.072m0 0l-2.829-2.829m-4.243 2.829a4.978 4.978 0 01-1.414-2.83m-1.414 5.658a9 9 0 01-2.167-9.238m7.824 2.167a1 1 0 111.414 1.414m-1.414-1.414L3 3m8.293 8.293l1.414 1.414"
              />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium text-foreground">You're offline</p>
            <p class="text-xs text-muted-foreground truncate">
              Changes will sync when you're back online
            </p>
          </div>
        {:else if $hasPendingSyncs}
          <div class="flex-shrink-0">
            <svg
              class="w-5 h-5 text-blue-500 animate-spin"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
              />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium text-foreground">Syncing changes...</p>
            <p class="text-xs text-muted-foreground">
              {$network.pendingSyncCount} pending
            </p>
          </div>
        {/if}

        <!-- Expand arrow -->
        <svg
          class="w-4 h-4 text-muted-foreground transition-transform"
          class:rotate-180={isExpanded}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      <!-- Expanded details -->
      {#if isExpanded}
        <div
          class="border-t border-border px-4 py-3 bg-muted/30"
          transition:slide={{ duration: 200 }}
        >
          <dl class="space-y-2 text-xs">
            <div class="flex justify-between">
              <dt class="text-muted-foreground">Status</dt>
              <dd class="font-medium">
                {#if $isOffline}
                  <span class="text-amber-500">Offline</span>
                {:else if $network.isSyncing}
                  <span class="text-blue-500">Syncing</span>
                {:else}
                  <span class="text-green-500">Online</span>
                {/if}
              </dd>
            </div>

            <div class="flex justify-between">
              <dt class="text-muted-foreground">Last synced</dt>
              <dd class="font-medium text-foreground">
                {formatTime($network.lastOnlineAt)}
              </dd>
            </div>

            {#if $network.pendingSyncCount > 0}
              <div class="flex justify-between">
                <dt class="text-muted-foreground">Pending changes</dt>
                <dd class="font-medium text-foreground">
                  {$network.pendingSyncCount}
                </dd>
              </div>
            {/if}

            {#if $network.effectiveType}
              <div class="flex justify-between">
                <dt class="text-muted-foreground">Connection</dt>
                <dd class="font-medium text-foreground uppercase">
                  {$network.effectiveType}
                </dd>
              </div>
            {/if}

            {#if $network.lastSyncError}
              <div class="mt-2 p-2 bg-destructive/10 rounded text-destructive">
                {$network.lastSyncError}
              </div>
            {/if}
          </dl>
        </div>
      {/if}
    </div>
  </div>
{/if}
