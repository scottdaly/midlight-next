<script lang="ts">
  /**
   * InlineEditPrompt - Floating prompt input for inline AI editing
   * Triggered by Cmd+K when text is selected in the editor
   */

  import { ai } from '@midlight/stores';

  interface Props {
    position: { x: number; y: number };
    selectedText: string;
    onSubmit: (instruction: string) => void;
    onCancel: () => void;
  }

  let { position, selectedText, onSubmit, onCancel }: Props = $props();

  let instruction = $state('');
  let inputRef: HTMLInputElement | null = $state(null);
  let isLoading = $state(false);

  // Focus input when mounted
  $effect(() => {
    if (inputRef) {
      inputRef.focus();
    }
  });

  function handleSubmit(e: Event) {
    e.preventDefault();
    if (!instruction.trim() || isLoading) return;

    isLoading = true;
    onSubmit(instruction.trim());
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    }
  }

  // Truncate selected text for display
  const displayText = $derived(() => {
    const text = selectedText.trim();
    if (text.length > 50) {
      return text.slice(0, 50) + '...';
    }
    return text;
  });
</script>

<div
  class="inline-edit-prompt fixed z-50 bg-popover border border-border rounded-lg shadow-xl p-3 min-w-[320px] max-w-[480px]"
  style="left: {position.x}px; top: {position.y}px;"
>
  <!-- Header -->
  <div class="flex items-center gap-2 mb-2">
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary">
      <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"/>
    </svg>
    <span class="text-xs font-medium text-foreground">Edit with AI</span>
    <span class="text-xs text-muted-foreground ml-auto">Esc to cancel</span>
  </div>

  <!-- Selected text preview -->
  <div class="mb-3 px-2 py-1.5 bg-muted/50 rounded text-xs text-muted-foreground font-mono truncate">
    "{displayText()}"
  </div>

  <!-- Input form -->
  <form onsubmit={handleSubmit}>
    <div class="flex gap-2">
      <input
        bind:this={inputRef}
        bind:value={instruction}
        onkeydown={handleKeyDown}
        type="text"
        placeholder="Describe the change... (e.g., 'make it more formal')"
        disabled={isLoading}
        class="flex-1 bg-background border border-input rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-50"
      />
      <button
        type="submit"
        disabled={!instruction.trim() || isLoading}
        class="px-3 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-1"
      >
        {#if isLoading}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="animate-spin">
            <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
          </svg>
        {:else}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21"/>
          </svg>
        {/if}
      </button>
    </div>
  </form>

  <!-- Quick suggestions -->
  <div class="mt-2 flex flex-wrap gap-1">
    {#each ['Fix grammar', 'Make shorter', 'Make longer', 'Simplify'] as suggestion}
      <button
        type="button"
        onclick={() => { instruction = suggestion; }}
        disabled={isLoading}
        class="px-2 py-0.5 text-xs rounded-full bg-muted hover:bg-accent text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
      >
        {suggestion}
      </button>
    {/each}
  </div>
</div>

<style>
  .inline-edit-prompt {
    animation: fadeIn 0.15s ease-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
