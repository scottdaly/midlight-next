<script lang="ts">
  import {
    sync,
    syncStatus,
    hasConflicts,
    conflictCount,
    lastSyncTime,
    syncUsage,
    auth,
    isAuthenticated,
  } from '@midlight/stores';
  import { slide } from 'svelte/transition';
  import type { SyncStatusType } from '@midlight/stores';

  let isExpanded = $state(false);

  function toggleExpand() {
    isExpanded = !isExpanded;
  }

  function formatRelativeTime(date: Date | null): string {
    if (!date) return 'Never';
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    if (seconds < 60) return 'Just now';
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    return date.toLocaleDateString();
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
  }

  function getStatusColor(status: SyncStatusType): string {
    switch (status) {
      case 'synced': return 'text-green-500';
      case 'syncing': return 'text-blue-500';
      case 'conflict': return 'text-amber-500';
      case 'error': return 'text-red-500';
      case 'offline': return 'text-gray-500';
      case 'disabled': return 'text-gray-400';
      case 'initializing': return 'text-blue-400';
      default: return 'text-muted-foreground';
    }
  }

  function getStatusText(status: SyncStatusType): string {
    switch (status) {
      case 'synced': return 'Synced';
      case 'syncing': return 'Syncing...';
      case 'conflict': return `${$conflictCount} conflict${$conflictCount > 1 ? 's' : ''}`;
      case 'error': return 'Sync error';
      case 'offline': return 'Offline';
      case 'disabled': return 'Sync disabled';
      case 'initializing': return 'Initializing...';
      default: return 'Unknown';
    }
  }

  function openConflict() {
    if ($sync.conflicts.length > 0) {
      sync.setActiveConflict($sync.conflicts[0]);
    }
  }

  async function triggerSync() {
    // This will be connected to the actual sync function
    if ($sync.enabled && !$sync.isSyncing) {
      // Trigger sync through the registered callback
      // The sync store handles this internally
    }
  }
</script>

{#if $isAuthenticated && $sync.enabled}
  <div class="relative">
    <button
      class="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md hover:bg-accent/50 transition-colors"
      onclick={toggleExpand}
      title="Sync Status"
    >
      <!-- Status icon -->
      {#if $syncStatus === 'syncing'}
        <svg class="w-4 h-4 animate-spin text-blue-500" fill="none" viewBox="0 0 24 24">
          <path class="opacity-25" fill="currentColor" d="M12 2a10 10 0 100 20 10 10 0 000-20z" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
        </svg>
      {:else if $syncStatus === 'synced'}
        <svg class="w-4 h-4 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
        </svg>
      {:else if $syncStatus === 'conflict'}
        <svg class="w-4 h-4 text-amber-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
        </svg>
      {:else if $syncStatus === 'error'}
        <svg class="w-4 h-4 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
      {:else if $syncStatus === 'offline'}
        <svg class="w-4 h-4 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18.364 5.636a9 9 0 010 12.728m0 0l-2.829-2.829m2.829 2.829L21 21" />
        </svg>
      {:else}
        <svg class="w-4 h-4 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      {/if}

      <span class={`text-xs font-medium ${getStatusColor($syncStatus)}`}>
        {getStatusText($syncStatus)}
      </span>

      <!-- Expand arrow -->
      <svg
        class="w-3 h-3 text-muted-foreground transition-transform"
        class:rotate-180={isExpanded}
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </button>

    <!-- Expanded dropdown -->
    {#if isExpanded}
      <div
        class="absolute right-0 top-full mt-1 w-64 bg-card border border-border rounded-lg shadow-lg overflow-hidden z-50"
        transition:slide={{ duration: 150 }}
      >
        <div class="p-4 space-y-4">
          <!-- Last sync time -->
          <div class="flex justify-between text-sm">
            <span class="text-muted-foreground">Last synced</span>
            <span class="font-medium text-foreground">
              {formatRelativeTime($lastSyncTime)}
            </span>
          </div>

          <!-- Storage usage -->
          {#if $syncUsage}
            <div>
              <div class="flex justify-between text-sm mb-1">
                <span class="text-muted-foreground">Storage</span>
                <span class="font-medium text-foreground">
                  {formatBytes($syncUsage.totalSizeBytes)} / {formatBytes($syncUsage.limitBytes)}
                </span>
              </div>
              <div class="h-1.5 bg-muted rounded-full overflow-hidden">
                <div
                  class="h-full transition-all"
                  class:bg-green-500={$syncUsage.percentUsed < 70}
                  class:bg-amber-500={$syncUsage.percentUsed >= 70 && $syncUsage.percentUsed < 90}
                  class:bg-red-500={$syncUsage.percentUsed >= 90}
                  style="width: {Math.min(100, $syncUsage.percentUsed)}%"
                />
              </div>
            </div>
          {/if}

          <!-- Conflicts -->
          {#if $hasConflicts}
            <button
              class="w-full flex items-center justify-between px-3 py-2 bg-amber-500/10 border border-amber-500/20 rounded-md text-amber-600 hover:bg-amber-500/20 transition-colors"
              onclick={openConflict}
            >
              <span class="text-sm font-medium">
                {$conflictCount} conflict{$conflictCount > 1 ? 's' : ''} to resolve
              </span>
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
            </button>
          {/if}

          <!-- Error message -->
          {#if $sync.lastSyncError}
            <div class="text-xs text-red-500 bg-red-500/10 px-3 py-2 rounded-md">
              {$sync.lastSyncError}
            </div>
          {/if}

          <!-- Document count -->
          <div class="flex justify-between text-sm">
            <span class="text-muted-foreground">Documents synced</span>
            <span class="font-medium text-foreground">
              {$syncUsage?.documentCount ?? 0}
            </span>
          </div>

          <!-- Manual sync button -->
          <button
            class="w-full px-3 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors disabled:opacity-50"
            onclick={triggerSync}
            disabled={$sync.isSyncing || $syncStatus === 'offline'}
          >
            {#if $sync.isSyncing}
              Syncing...
            {:else}
              Sync Now
            {/if}
          </button>
        </div>
      </div>
    {/if}
  </div>
{/if}
