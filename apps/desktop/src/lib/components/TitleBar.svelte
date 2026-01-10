<script lang="ts">
  import WindowsMenu from './WindowsMenu.svelte';
  import WindowControls from './WindowControls.svelte';
  import SearchBar from './SearchBar.svelte';

  // Platform detection
  const isMac = navigator.userAgent.includes('Mac');
  const isWindows = navigator.userAgent.includes('Windows');
</script>

<div
  class="h-10 bg-secondary flex items-center select-none relative shrink-0"
>
  <!-- Drag Region (Background Layer) - Offset on Mac to avoid traffic lights -->
  <div
    class="absolute inset-0 {isMac ? 'left-20' : ''}"
    data-tauri-drag-region
  ></div>

  <!-- Content Layer -->
  <div
    class="relative z-10 w-full h-full flex items-center px-2 pointer-events-none
    {isMac ? 'pl-20' : ''}"
  >
    <!-- Menu (Windows/Linux only) -->
    {#if !isMac}
      <div class="pointer-events-auto">
        <WindowsMenu />
      </div>
    {/if}

    <!-- Spacer -->
    <div class="flex-1"></div>

    <!-- Window Controls (Windows only) -->
    {#if isWindows}
      <div class="pointer-events-auto">
        <WindowControls />
      </div>
    {/if}
  </div>

  <!-- Centered Search Bar Layer -->
  <div class="absolute inset-0 flex items-center justify-center pointer-events-none z-20 pt-1">
    <div class="pointer-events-auto">
      <SearchBar />
    </div>
  </div>
</div>
