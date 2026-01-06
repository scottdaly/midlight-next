<script lang="ts">
  import { ai } from '@midlight/stores';

  interface Props {
    onClose?: () => void;
  }

  let { onClose }: Props = $props();

  let isEditing = $state(false);
  let editTitle = $state('');
  let editInputRef = $state<HTMLInputElement | null>(null);
  let showHistory = $state(false);

  // Get active conversation
  const activeConversation = $derived(
    $ai.conversations.find((c) => c.id === $ai.activeConversationId)
  );

  // Get other conversations for history dropdown
  const otherConversations = $derived(
    $ai.conversations.filter((c) => c.id !== $ai.activeConversationId)
  );

  function handleStartRename() {
    if (activeConversation) {
      editTitle = activeConversation.title;
      isEditing = true;
      // Focus input after render
      setTimeout(() => {
        editInputRef?.focus();
        editInputRef?.select();
      }, 0);
    }
  }

  function handleSaveRename() {
    if ($ai.activeConversationId && editTitle.trim()) {
      ai.updateConversationTitle($ai.activeConversationId, editTitle.trim());
    }
    isEditing = false;
    editTitle = '';
  }

  function handleCancelRename() {
    isEditing = false;
    editTitle = '';
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      handleSaveRename();
    } else if (e.key === 'Escape') {
      handleCancelRename();
    }
  }

  function handleNewChat() {
    // Don't create a new conversation if the current one is empty
    if (activeConversation && activeConversation.messages.length === 0) {
      return;
    }
    ai.createConversation();
  }

  function handleSwitchConversation(id: string) {
    ai.setActiveConversation(id);
    showHistory = false;
  }

  function handleDeleteConversation(e: MouseEvent, id: string) {
    e.stopPropagation();
    ai.deleteConversation(id);
  }

  function handleClickOutside() {
    showHistory = false;
  }
</script>

<svelte:window onclick={handleClickOutside} />

<div class="flex items-center gap-2 px-3 py-2 border-b border-border">
  <!-- Title - click to edit -->
  <div class="flex-1 min-w-0">
    {#if isEditing}
      <input
        bind:this={editInputRef}
        type="text"
        bind:value={editTitle}
        onkeydown={handleKeyDown}
        onblur={handleSaveRename}
        class="w-full px-2 py-0.5 text-sm font-medium bg-background border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary"
      />
    {:else}
      <button
        class="font-medium text-sm truncate cursor-pointer hover:text-primary transition-colors text-left w-full"
        onclick={handleStartRename}
        title={activeConversation?.title || 'New chat'}
      >
        {activeConversation?.title || 'New chat'}
      </button>
    {/if}
  </div>

  <!-- Action buttons -->
  <div class="flex items-center gap-0.5 flex-shrink-0">
    <!-- New chat -->
    <button
      onclick={handleNewChat}
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
      title="New chat"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="12" y1="5" x2="12" y2="19"/>
        <line x1="5" y1="12" x2="19" y2="12"/>
      </svg>
    </button>

    <!-- History dropdown -->
    <div class="relative">
      <button
        onclick={(e) => { e.stopPropagation(); showHistory = !showHistory; }}
        class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
        title="Chat history"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="8" y1="6" x2="21" y2="6"/>
          <line x1="8" y1="12" x2="21" y2="12"/>
          <line x1="8" y1="18" x2="21" y2="18"/>
          <line x1="3" y1="6" x2="3.01" y2="6"/>
          <line x1="3" y1="12" x2="3.01" y2="12"/>
          <line x1="3" y1="18" x2="3.01" y2="18"/>
        </svg>
      </button>

      {#if showHistory}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="absolute right-0 top-full mt-1 w-56 bg-popover border border-border rounded-md shadow-lg py-1 z-50"
          onclick={(e) => e.stopPropagation()}
        >
          {#if otherConversations.length === 0}
            <div class="px-3 py-3 text-xs text-muted-foreground text-center">
              No previous chats
            </div>
          {:else}
            <div class="px-3 py-1.5 text-xs font-medium text-muted-foreground">
              Previous Chats
            </div>
            {#each otherConversations as conversation (conversation.id)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                onclick={() => handleSwitchConversation(conversation.id)}
                class="w-full flex items-center justify-between gap-2 px-3 py-1.5 text-sm hover:bg-accent cursor-pointer text-left"
              >
                <span class="truncate flex-1">
                  {conversation.title}
                </span>
                <button
                  onclick={(e) => handleDeleteConversation(e, conversation.id)}
                  class="p-1 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors"
                  title="Delete chat"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M3 6h18"/>
                    <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>
                    <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>
                  </svg>
                </button>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <!-- Close -->
    {#if onClose}
      <button
        onclick={onClose}
        class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
        title="Close"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18"/>
          <line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
      </button>
    {/if}
  </div>
</div>
