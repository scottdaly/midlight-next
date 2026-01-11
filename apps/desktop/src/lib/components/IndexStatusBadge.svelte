<script lang="ts">
  import { rag } from '@midlight/stores';
  import type { IndexStatus } from '@midlight/core';

  interface Props {
    projectPath: string;
  }

  let { projectPath }: Props = $props();

  // Get index status for this project from RAG store
  const status = $derived.by(() => {
    const ragState = $rag;
    return ragState.indexStatus.get(projectPath) as IndexStatus | undefined;
  });

  // Determine badge state
  const badgeState = $derived.by(() => {
    if (!status) return 'not-indexed';
    if (status.isIndexing) return 'indexing';
    if (status.error) return 'error';
    if (status.totalChunks > 0) return 'indexed';
    return 'not-indexed';
  });

  // Format last indexed time
  function formatLastIndexed(isoDate?: string): string {
    if (!isoDate) return 'Never';
    const date = new Date(isoDate);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  }
</script>

{#if badgeState === 'indexing'}
  <!-- Indexing spinner -->
  <span class="inline-flex items-center" title="Indexing for semantic search...">
    <svg class="animate-spin w-3 h-3 text-primary" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
    </svg>
  </span>
{:else if badgeState === 'indexed'}
  <!-- Indexed checkmark -->
  <span
    class="inline-flex items-center group relative"
    title={`Indexed: ${status?.totalChunks} chunks from ${status?.indexedDocuments} docs\nLast indexed: ${formatLastIndexed(status?.lastIndexed)}`}
  >
    <svg class="w-3 h-3 text-green-500" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="11" cy="11" r="8"/>
      <path d="m21 21-4.3-4.3"/>
      <path d="m8 11 2 2 4-4"/>
    </svg>
  </span>
{:else if badgeState === 'error'}
  <!-- Error indicator -->
  <span
    class="inline-flex items-center"
    title={`Index error: ${status?.error}`}
  >
    <svg class="w-3 h-3 text-destructive" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="10"/>
      <line x1="12" y1="8" x2="12" y2="12"/>
      <line x1="12" y1="16" x2="12.01" y2="16"/>
    </svg>
  </span>
{:else}
  <!-- Not indexed - subtle indicator -->
  <span
    class="inline-flex items-center opacity-30 hover:opacity-60 transition-opacity"
    title="Not indexed for semantic search"
  >
    <svg class="w-3 h-3 text-muted-foreground" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="11" cy="11" r="8"/>
      <path d="m21 21-4.3-4.3"/>
    </svg>
  </span>
{/if}
