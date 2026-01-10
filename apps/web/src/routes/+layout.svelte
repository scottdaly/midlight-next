<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { settings } from '@midlight/stores';
  import OfflineIndicator from '$lib/components/OfflineIndicator.svelte';
  import InstallBanner from '$lib/components/InstallBanner.svelte';

  let { children } = $props();

  // Apply theme class to document
  onMount(() => {
    const unsubscribe = settings.subscribe(($settings) => {
      const root = document.documentElement;

      if ($settings.theme === 'system') {
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        root.classList.toggle('dark', prefersDark);
      } else if ($settings.theme === 'dark' || $settings.theme === 'midnight' || $settings.theme === 'cyberpunk') {
        root.classList.add('dark');
      } else {
        root.classList.remove('dark');
      }
    });

    return unsubscribe;
  });
</script>

<!-- Global UI Components -->
<OfflineIndicator />
<InstallBanner />

<div class="min-h-screen bg-background text-foreground">
  {@render children()}
</div>
