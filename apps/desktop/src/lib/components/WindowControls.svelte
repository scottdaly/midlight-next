<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';

  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);

  onMount(() => {
    // Initial check
    appWindow.isMaximized().then(val => isMaximized = val);
    
    // Listen for resize events
    let unlisten: () => void;
    appWindow.onResized(async () => {
      isMaximized = await appWindow.isMaximized();
    }).then(u => unlisten = u);

    return () => {
      if (unlisten) unlisten();
    };
  });

  async function toggleMaximize() {
    if (isMaximized) {
      await appWindow.unmaximize();
    } else {
      await appWindow.maximize();
    }
  }
</script>

<div class="flex h-10 -mr-2 window-controls">
  <button
    class="inline-flex justify-center items-center w-12 h-10 hover:bg-white/10 active:bg-white/20 transition-colors"
    onclick={() => appWindow.minimize()}
    title="Minimize"
    tabindex="-1"
  >
    <svg width="10" height="1" viewBox="0 0 10 1" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M0 0.5H10" stroke="currentColor" stroke-width="1"/>
    </svg>
  </button>

  <button
    class="inline-flex justify-center items-center w-12 h-10 hover:bg-white/10 active:bg-white/20 transition-colors"
    onclick={toggleMaximize}
    title={isMaximized ? "Restore" : "Maximize"}
    tabindex="-1"
  >
    {#if isMaximized}
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M2.5 2.5H9.5V9.5H2.5V2.5Z" stroke="currentColor" stroke-width="1"/>
        <path d="M0.5 0.5H7.5V7.5H0.5V0.5Z" stroke="currentColor" stroke-width="1" fill="transparent"/>
        <!-- Adjusted restore icon path for better clarity -->
         <path d="M2.5 2.5V0.5H9.5V7.5H7.5" stroke="currentColor" stroke-width="1"/>
      </svg>
    {:else}
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
        <rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" stroke-width="1"/>
      </svg>
    {/if}
  </button>

  <button
    class="inline-flex justify-center items-center w-12 h-10 hover:bg-red-500 active:bg-red-600 transition-colors hover:text-white"
    onclick={() => appWindow.close()}
    title="Close"
    tabindex="-1"
  >
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M0.5 0.5L9.5 9.5" stroke="currentColor" stroke-width="1"/>
      <path d="M9.5 0.5L0.5 9.5" stroke="currentColor" stroke-width="1"/>
    </svg>
  </button>
</div>
