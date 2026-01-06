<script lang="ts">
  /**
   * AnnotationPopover - Shows details about an AI annotation
   * Appears when hovering or clicking on annotated text
   */

  import { activeConversation, ai } from '@midlight/stores';

  interface Props {
    position: { x: number; y: number };
    conversationId: string;
    messageId: string;
    type: 'edit' | 'suggestion' | 'reference';
    tooltip?: string;
    onClose: () => void;
    onGoToConversation?: () => void;
  }

  let {
    position,
    conversationId,
    messageId,
    type,
    tooltip,
    onClose,
    onGoToConversation,
  }: Props = $props();

  // Get the type label and icon color
  const typeConfig = $derived({
    edit: { label: 'AI Edit', color: 'text-blue-400', bgColor: 'bg-blue-500/20' },
    suggestion: { label: 'AI Suggestion', color: 'text-amber-400', bgColor: 'bg-amber-500/20' },
    reference: { label: 'AI Reference', color: 'text-purple-400', bgColor: 'bg-purple-500/20' },
  }[type] || { label: 'AI Annotation', color: 'text-blue-400', bgColor: 'bg-blue-500/20' });

  // Find the message in the conversation
  const message = $derived(() => {
    const conversation = $activeConversation;
    if (!conversation || conversation.id !== conversationId) return null;
    return conversation.messages.find((m) => m.id === messageId);
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function handleGoToConversation() {
    if (conversationId) {
      ai.setActiveConversation(conversationId);
      onGoToConversation?.();
    }
    onClose();
  }

  function handleRemoveAnnotation() {
    // This will be implemented when we wire up the editor
    onClose();
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="annotation-popover fixed z-50 bg-popover border border-border rounded-lg shadow-xl min-w-[240px] max-w-[320px]"
  style="left: {position.x}px; top: {position.y}px;"
  onclick={(e) => e.stopPropagation()}
>
  <!-- Header -->
  <div class="flex items-center gap-2 px-3 py-2 border-b border-border">
    <div class="flex items-center gap-2">
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={typeConfig.color}>
        <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z"/>
      </svg>
      <span class="text-xs font-medium {typeConfig.color}">{typeConfig.label}</span>
    </div>
    <button
      onclick={onClose}
      class="ml-auto p-0.5 hover:bg-accent rounded text-muted-foreground hover:text-foreground"
      title="Close"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="18" y1="6" x2="6" y2="18"/>
        <line x1="6" y1="6" x2="18" y2="18"/>
      </svg>
    </button>
  </div>

  <!-- Content -->
  <div class="p-3">
    {#if tooltip}
      <p class="text-sm text-foreground mb-3">{tooltip}</p>
    {:else}
      <p class="text-sm text-muted-foreground mb-3 italic">
        {#if type === 'edit'}
          This text was edited by AI
        {:else if type === 'suggestion'}
          This text was suggested by AI
        {:else}
          This text was referenced by AI
        {/if}
      </p>
    {/if}

    {#if message()}
      <div class="text-xs text-muted-foreground mb-3">
        <span>From conversation at </span>
        <span class="text-foreground">{new Date(message()!.timestamp).toLocaleString()}</span>
      </div>
    {/if}
  </div>

  <!-- Actions -->
  <div class="flex items-center gap-2 px-3 py-2 border-t border-border bg-muted/30">
    {#if conversationId}
      <button
        onclick={handleGoToConversation}
        class="flex-1 px-2 py-1 text-xs rounded bg-accent hover:bg-accent/80 text-foreground transition-colors flex items-center justify-center gap-1"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
        </svg>
        View Chat
      </button>
    {/if}
    <button
      onclick={handleRemoveAnnotation}
      class="flex-1 px-2 py-1 text-xs rounded bg-muted hover:bg-muted/80 text-muted-foreground hover:text-foreground transition-colors flex items-center justify-center gap-1"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="18" y1="6" x2="6" y2="18"/>
        <line x1="6" y1="6" x2="18" y2="18"/>
      </svg>
      Remove
    </button>
  </div>
</div>

<style>
  .annotation-popover {
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
