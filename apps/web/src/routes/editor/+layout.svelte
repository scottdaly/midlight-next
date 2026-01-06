<script lang="ts">
  import { onMount } from 'svelte';
  import { fileSystem } from '@midlight/stores';
  import { WebStorageAdapter } from '$lib/storage/adapter';

  let { children } = $props();
  let initialized = $state(false);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      // Initialize the web storage adapter
      const adapter = new WebStorageAdapter();
      await adapter.init();
      fileSystem.setStorageAdapter(adapter);
      initialized = true;
    } catch (e) {
      console.error('Failed to initialize storage:', e);
      error = e instanceof Error ? e.message : 'Failed to initialize storage';
    }
  });
</script>

{#if error}
  <div class="flex items-center justify-center min-h-screen p-8">
    <div class="max-w-md text-center space-y-4">
      <h2 class="text-2xl font-bold text-destructive">Storage Error</h2>
      <p class="text-muted-foreground">{error}</p>
      <p class="text-sm text-muted-foreground">
        Your browser may not support the required storage APIs (OPFS).
        Please try a modern browser like Chrome or Edge.
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
