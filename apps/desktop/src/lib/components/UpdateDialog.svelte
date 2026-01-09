<script lang="ts">
  import {
    updateStore,
    showUpdateDialog,
    availableUpdate,
    updateStatus,
    downloadProgress,
    updateError,
  } from '@midlight/stores';
  import { updatesClient } from '$lib/updates';

  let dialogRef: HTMLDivElement | null = $state(null);

  // Derived state
  const isDownloading = $derived($updateStatus === 'downloading');
  const isReady = $derived($updateStatus === 'ready');
  const hasError = $derived($updateStatus === 'error');

  // Format release notes (basic markdown to text)
  function formatNotes(notes?: string): string {
    if (!notes) return 'Bug fixes and performance improvements.';
    // Basic markdown stripping
    return notes
      .replace(/#{1,6}\s/g, '') // Remove headers
      .replace(/\*\*/g, '') // Remove bold
      .replace(/\*/g, '') // Remove italic
      .replace(/`/g, '') // Remove code
      .trim();
  }

  // Handle install now
  async function handleInstall() {
    await updatesClient.downloadAndInstall();
  }

  // Handle remind later
  function handleLater() {
    updateStore.dismissUpdate();
  }

  // Handle close
  function handleClose() {
    updateStore.closeDialog();
  }

  // Handle restart to apply update
  function handleRestart() {
    // The update is installed, restart will apply it
    // Tauri handles this automatically via the updater plugin
    window.location.reload();
  }

  // Close on escape key
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !isDownloading) {
      handleClose();
    }
  }

  // Focus trap
  $effect(() => {
    if ($showUpdateDialog && dialogRef) {
      dialogRef.focus();
    }
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $showUpdateDialog && $availableUpdate}
  <!-- Backdrop -->
  <div class="fixed inset-0 bg-black/50 z-50" role="presentation">
    <!-- Dialog -->
    <div
      bind:this={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="update-title"
      tabindex="-1"
      class="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2
             w-[420px] max-h-[90vh] overflow-auto
             bg-white dark:bg-zinc-900 rounded-xl shadow-2xl
             border border-zinc-200 dark:border-zinc-700"
    >
      <!-- Header -->
      <div class="p-6 pb-4 border-b border-zinc-200 dark:border-zinc-700">
        <div class="flex items-start gap-4">
          <!-- Update icon -->
          <div
            class="w-12 h-12 rounded-xl bg-blue-100 dark:bg-blue-900/30
                      flex items-center justify-center flex-shrink-0"
          >
            <svg
              class="w-6 h-6 text-blue-600 dark:text-blue-400"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
              />
            </svg>
          </div>

          <div class="flex-1 min-w-0">
            <h2
              id="update-title"
              class="text-lg font-semibold text-zinc-900 dark:text-zinc-100"
            >
              Update Available
            </h2>
            <p class="text-sm text-zinc-600 dark:text-zinc-400 mt-1">
              Version {$availableUpdate.version} is ready to install
            </p>
          </div>

          {#if !isDownloading}
            <button
              onclick={handleClose}
              class="p-1 rounded-lg hover:bg-zinc-100 dark:hover:bg-zinc-800
                     text-zinc-500 dark:text-zinc-400 transition-colors"
              aria-label="Close"
            >
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          {/if}
        </div>
      </div>

      <!-- Content -->
      <div class="p-6">
        <!-- Version info -->
        <div class="flex items-center gap-2 text-sm mb-4">
          <span class="text-zinc-500 dark:text-zinc-400">
            {$availableUpdate.currentVersion}
          </span>
          <svg class="w-4 h-4 text-zinc-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7l5 5m0 0l-5 5m5-5H6" />
          </svg>
          <span class="font-medium text-zinc-900 dark:text-zinc-100">
            {$availableUpdate.version}
          </span>
        </div>

        <!-- Release notes -->
        {#if $availableUpdate.body}
          <div class="mb-4">
            <h3 class="text-sm font-medium text-zinc-700 dark:text-zinc-300 mb-2">
              What's New
            </h3>
            <div
              class="text-sm text-zinc-600 dark:text-zinc-400
                        bg-zinc-50 dark:bg-zinc-800/50 rounded-lg p-3
                        max-h-32 overflow-y-auto"
            >
              {formatNotes($availableUpdate.body)}
            </div>
          </div>
        {/if}

        <!-- Download progress -->
        {#if isDownloading}
          <div class="mb-4">
            <div class="flex items-center justify-between text-sm mb-2">
              <span class="text-zinc-600 dark:text-zinc-400">Downloading...</span>
              <span class="text-zinc-900 dark:text-zinc-100 font-medium">
                {$downloadProgress}%
              </span>
            </div>
            <div class="h-2 bg-zinc-200 dark:bg-zinc-700 rounded-full overflow-hidden">
              <div
                class="h-full bg-blue-500 transition-all duration-300 ease-out"
                style="width: {$downloadProgress}%"
              ></div>
            </div>
          </div>
        {/if}

        <!-- Ready to install -->
        {#if isReady}
          <div
            class="mb-4 p-3 bg-green-50 dark:bg-green-900/20 rounded-lg
                      border border-green-200 dark:border-green-800"
          >
            <div class="flex items-center gap-2">
              <svg
                class="w-5 h-5 text-green-600 dark:text-green-400"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
              </svg>
              <span class="text-sm font-medium text-green-700 dark:text-green-300">
                Update downloaded! Restart to apply.
              </span>
            </div>
          </div>
        {/if}

        <!-- Error -->
        {#if hasError && $updateError}
          <div
            class="mb-4 p-3 bg-red-50 dark:bg-red-900/20 rounded-lg
                      border border-red-200 dark:border-red-800"
          >
            <div class="flex items-start gap-2">
              <svg
                class="w-5 h-5 text-red-600 dark:text-red-400 flex-shrink-0 mt-0.5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              <div>
                <span class="text-sm font-medium text-red-700 dark:text-red-300">
                  Update failed
                </span>
                <p class="text-xs text-red-600 dark:text-red-400 mt-1">
                  {$updateError}
                </p>
              </div>
            </div>
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div
        class="p-4 border-t border-zinc-200 dark:border-zinc-700
                  flex items-center justify-end gap-3"
      >
        {#if isReady}
          <button
            onclick={handleLater}
            class="px-4 py-2 text-sm font-medium text-zinc-600 dark:text-zinc-400
                   hover:bg-zinc-100 dark:hover:bg-zinc-800 rounded-lg transition-colors"
          >
            Later
          </button>
          <button
            onclick={handleRestart}
            class="px-4 py-2 text-sm font-medium text-white
                   bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors"
          >
            Restart Now
          </button>
        {:else if isDownloading}
          <button
            disabled
            class="px-4 py-2 text-sm font-medium text-zinc-400 dark:text-zinc-500
                   bg-zinc-100 dark:bg-zinc-800 rounded-lg cursor-not-allowed"
          >
            Downloading...
          </button>
        {:else if hasError}
          <button
            onclick={handleClose}
            class="px-4 py-2 text-sm font-medium text-zinc-600 dark:text-zinc-400
                   hover:bg-zinc-100 dark:hover:bg-zinc-800 rounded-lg transition-colors"
          >
            Dismiss
          </button>
          <button
            onclick={handleInstall}
            class="px-4 py-2 text-sm font-medium text-white
                   bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors"
          >
            Try Again
          </button>
        {:else}
          <button
            onclick={handleLater}
            class="px-4 py-2 text-sm font-medium text-zinc-600 dark:text-zinc-400
                   hover:bg-zinc-100 dark:hover:bg-zinc-800 rounded-lg transition-colors"
          >
            Remind Later
          </button>
          <button
            onclick={handleInstall}
            class="px-4 py-2 text-sm font-medium text-white
                   bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors"
          >
            Install Update
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
