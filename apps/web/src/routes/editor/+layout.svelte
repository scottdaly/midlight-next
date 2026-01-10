<script lang="ts">
  import { onMount } from 'svelte';
  import { fileSystem, isAuthenticated } from '@midlight/stores';
  import { createStorageAdapter, getStorageTypeDescription, type StorageType } from '$lib/storage/factory';
  import { initSyncIntegration, destroySyncIntegration } from '$lib/sync/integration';
  import ConflictDialog from '$lib/components/ConflictDialog.svelte';

  let { children } = $props();
  let initialized = $state(false);
  let error = $state<string | null>(null);
  let storageType = $state<StorageType | null>(null);
  let syncInitialized = $state(false);

  onMount(() => {
    // Initialize storage asynchronously
    (async () => {
      try {
        // Initialize storage using the factory (uses OPFS with IndexedDB fallback)
        const { adapter, type } = await createStorageAdapter();
        storageType = type;
        fileSystem.setStorageAdapter(adapter);
        console.log(`[Editor] Storage initialized: ${getStorageTypeDescription(type)}`);

        initialized = true;
      } catch (e) {
        console.error('Failed to initialize storage:', e);
        error = e instanceof Error ? e.message : 'Failed to initialize storage';
      }
    })();

    // Return cleanup function
    return () => {
      destroySyncIntegration();
    };
  });

  // Watch for auth changes to initialize/destroy sync (after storage is ready)
  $effect(() => {
    if (!initialized) return;

    if ($isAuthenticated && !syncInitialized) {
      initSyncIntegration();
      syncInitialized = true;
    } else if (!$isAuthenticated && syncInitialized) {
      destroySyncIntegration();
      syncInitialized = false;
    }
  });
</script>

<!-- Sync Conflict Resolution Dialog -->
<ConflictDialog />

{#if error}
  <div class="flex items-center justify-center min-h-screen p-8">
    <div class="max-w-md text-center space-y-4">
      <h2 class="text-2xl font-bold text-destructive">Storage Error</h2>
      <p class="text-muted-foreground">{error}</p>
      <p class="text-sm text-muted-foreground">
        {#if storageType === 'indexeddb'}
          Using fallback storage (IndexedDB). Some features may be limited.
        {:else}
          Your browser may not support the required storage APIs.
          Please try a modern browser like Chrome or Edge.
        {/if}
      </p>
    </div>
  </div>
{:else if !initialized}
  <div class="flex items-center justify-center min-h-screen">
    <div class="text-center space-y-4">
      <div class="w-8 h-8 border-4 border-primary border-t-transparent rounded-full animate-spin mx-auto"></div>
      <p class="text-muted-foreground">Initializing editor...</p>
    </div>
  </div>
{:else}
  {@render children()}
{/if}
