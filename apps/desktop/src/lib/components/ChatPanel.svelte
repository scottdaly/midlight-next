<script lang="ts">
  import { ai, activeConversation, fileSystem, auth } from '@midlight/stores';
  import type { LLMProvider } from '@midlight/core';
  import type { FileNode } from '@midlight/core/types';
  import { Markdown } from '@midlight/ui';
  import ConversationTabs from './Chat/ConversationTabs.svelte';
  import ContextPicker from './Chat/ContextPicker.svelte';
  import ContextPills from './Chat/ContextPills.svelte';
  import ToolActionsGroup from './Chat/ToolActionsGroup.svelte';
  import ThinkingSteps from './Chat/ThinkingSteps.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { authClient } from '$lib/auth';

  interface Props {
    onOpenAuth?: () => void;
    onClose?: () => void;
  }

  let { onOpenAuth, onClose }: Props = $props();

  let inputValue = $state('');
  let messagesContainer: HTMLDivElement;
  let textareaElement: HTMLTextAreaElement;
  let contextPicker: ContextPicker | null = $state(null);
  let showModelSelector = $state(false);
  let showContextPicker = $state(false);
  let contextQuery = $state('');
  let mentionStartIndex = $state(-1);

  // Model types from API
  interface ModelInfo {
    id: string;
    name: string;
    tier: string;
  }

  interface AvailableModels {
    openai: ModelInfo[];
    anthropic: ModelInfo[];
    gemini: ModelInfo[];
  }

  // Available models from API
  let availableModels = $state<AvailableModels | null>(null);
  let modelsLoading = $state(false);
  let modelsError = $state<string | null>(null);

  // Fallback models (used when API fails or still loading)
  const fallbackModels: Record<LLMProvider, { id: string; name: string }[]> = {
    anthropic: [
      { id: 'claude-haiku-4-5-20251001', name: 'Claude Haiku 4.5' },
    ],
    openai: [
      { id: 'gpt-5-nano', name: 'GPT-5 Nano' },
    ],
    gemini: [
      { id: 'gemini-3-flash-preview', name: 'Gemini 3 Flash' },
    ],
  };

  // Compute models to display (API models or fallback)
  const models = $derived(
    availableModels
      ? {
          anthropic: availableModels.anthropic.map((m: ModelInfo) => ({ id: m.id, name: m.name })),
          openai: availableModels.openai.map((m: ModelInfo) => ({ id: m.id, name: m.name })),
          gemini: availableModels.gemini.map((m: ModelInfo) => ({ id: m.id, name: m.name })),
        }
      : fallbackModels
  );

  // Fetch models when authenticated
  $effect(() => {
    if ($auth.isAuthenticated && !availableModels && !modelsLoading) {
      fetchModels();
    }
  });

  async function fetchModels() {
    modelsLoading = true;
    modelsError = null;
    try {
      const token = await authClient.getAccessToken();
      const fetchedModels = await invoke<AvailableModels>('llm_get_models', { authToken: token });
      availableModels = fetchedModels;

      // If current model is not in the new list, select the first available
      const currentProvider = $ai.selectedProvider;
      const providerModels = fetchedModels[currentProvider] || [];
      if (providerModels.length > 0 && !providerModels.find(m => m.id === $ai.selectedModel)) {
        ai.setModel(providerModels[0].id);
      }
    } catch (error) {
      console.error('Failed to fetch models:', error);
      modelsError = error instanceof Error ? error.message : String(error);
    } finally {
      modelsLoading = false;
    }
  }

  // Get current model display name
  const currentModelName = $derived.by(() => {
    const providerModels = models[$ai.selectedProvider];
    const model = providerModels?.find((m: { id: string; name: string }) => m.id === $ai.selectedModel);
    return model?.name ?? $ai.selectedModel;
  });

  function selectModel(provider: LLMProvider, modelId: string) {
    ai.setProvider(provider);
    ai.setModel(modelId);
    showModelSelector = false;
  }

  function handleWindowClick() {
    showModelSelector = false;
    showContextPicker = false;
  }

  // Handle input changes to detect @ mentions
  function handleInput(e: Event) {
    const textarea = e.target as HTMLTextAreaElement;
    const value = textarea.value;
    const cursorPos = textarea.selectionStart ?? value.length;

    // Auto-resize textarea
    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 120) + 'px';

    // Look for @ before cursor position
    const beforeCursor = value.slice(0, cursorPos);
    const atIndex = beforeCursor.lastIndexOf('@');

    if (atIndex !== -1) {
      // Check if there's a space between @ and cursor (means mention is complete)
      const afterAt = beforeCursor.slice(atIndex + 1);
      if (!afterAt.includes(' ')) {
        showContextPicker = true;
        mentionStartIndex = atIndex;
        contextQuery = afterAt;
        return;
      }
    }

    showContextPicker = false;
    mentionStartIndex = -1;
    contextQuery = '';
  }

  // Handle keyboard events in input
  function handleInputKeyDown(e: KeyboardEvent) {
    // Don't handle Enter when context picker is open
    if (showContextPicker) {
      if (e.key === 'Escape') {
        e.preventDefault();
        showContextPicker = false;
        contextQuery = '';
        mentionStartIndex = -1;
        return;
      }
      if (contextPicker) {
        const handled = contextPicker.handleKeyDown(e);
        if (handled) return;
      }
    }

    // Submit on Enter (without shift)
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  }

  // Handle file selection from context picker
  async function handleFileSelect(file: FileNode) {
    showContextPicker = false;

    // Load file content
    try {
      const content = await invoke<string>('read_file', { path: file.path });

      // Get display name
      let displayName = file.name;
      if (displayName.endsWith('.midlight')) {
        displayName = displayName.slice(0, -9);
      }

      // Add to context items
      ai.addContextItem({
        type: 'file',
        path: file.path,
        content,
        label: displayName,
      });

      // Remove the @query from input
      if (mentionStartIndex !== -1) {
        const before = inputValue.slice(0, mentionStartIndex);
        const cursorPos = textareaElement?.selectionStart ?? inputValue.length;
        const after = inputValue.slice(cursorPos);
        inputValue = before + after;
      }
    } catch (error) {
      console.error('Failed to load file:', error);
    }

    mentionStartIndex = -1;
    contextQuery = '';
    textareaElement?.focus();
  }

  function closeContextPicker() {
    showContextPicker = false;
    mentionStartIndex = -1;
    contextQuery = '';
    textareaElement?.focus();
  }

  function handleAtButtonClick() {
    // Insert @ at cursor position and trigger picker
    const cursorPos = textareaElement?.selectionStart || inputValue.length;
    inputValue = inputValue.substring(0, cursorPos) + '@' + inputValue.substring(cursorPos);
    showContextPicker = true;
    contextQuery = '';
    mentionStartIndex = cursorPos;
    textareaElement?.focus();
  }

  // Derived: get messages from active conversation
  const messages = $derived($activeConversation?.messages ?? []);

  // Check if there's an active document
  const hasActiveDocument = $derived(!!$fileSystem.activeFilePath);

  // Scroll to bottom when new messages arrive
  $effect(() => {
    if (messagesContainer && messages.length > 0) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  });

  // Ensure there's always an active conversation
  $effect(() => {
    if (!$ai.activeConversationId) {
      ai.createConversation('New Chat');
    }
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!inputValue.trim() || $ai.isStreaming) return;

    const message = inputValue.trim();
    inputValue = '';

    // Reset textarea height
    if (textareaElement) {
      textareaElement.style.height = 'auto';
    }

    // Use agent mode if enabled, otherwise regular chat
    if ($ai.agentEnabled) {
      await ai.sendMessageWithAgent(message);
    } else {
      await ai.sendMessage(message);
    }
  }

  function toggleAgentMode() {
    ai.setAgentEnabled(!$ai.agentEnabled);
  }

  function handleCancel() {
    ai.cancelStream();
  }
</script>

<svelte:window onclick={handleWindowClick} />

{#if !$auth.isAuthenticated}
  <!-- Sign-in prompt -->
  <div class="h-full flex flex-col">
    <!-- Header -->
    <div class="flex items-center justify-between px-3 py-2 border-b border-border">
      <span class="text-sm font-medium">New chat</span>
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

    <!-- Sign-in content -->
    <div class="flex-1 flex flex-col items-center justify-center p-6 text-center">
      <div class="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center mb-4">
        <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary">
          <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
          <polyline points="10 17 15 12 10 7"/>
          <line x1="15" y1="12" x2="3" y2="12"/>
        </svg>
      </div>
      <h3 class="font-medium text-sm mb-2">Sign in to use AI</h3>
      <p class="text-xs text-muted-foreground mb-4">
        Create an account or sign in to access AI writing assistance.
      </p>
      {#if onOpenAuth}
        <button
          onclick={onOpenAuth}
          class="px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:bg-primary/90 transition-colors"
        >
          Sign In
        </button>
      {/if}
    </div>
  </div>
{:else}
  <div class="h-full flex flex-col">
    <!-- Header with ConversationTabs -->
    <ConversationTabs {onClose} />

    <!-- Messages -->
    <div bind:this={messagesContainer} class="flex-1 overflow-auto px-4 pt-4 pb-2 space-y-4">
      {#if messages.length === 0}
        <!-- Empty state -->
        <div class="flex flex-col items-center justify-center h-full text-center text-muted-foreground">
          <svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="mb-3 opacity-50">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
          </svg>
          <p class="text-sm font-medium">Start a conversation</p>
          <p class="text-xs mt-1">Ask me to help with your writing</p>
          {#if hasActiveDocument}
            <p class="text-xs mt-3 px-4 py-2 bg-muted/50 rounded-lg">
              I can see your current document and help with editing, brainstorming, and more.
            </p>
          {/if}
        </div>
      {:else}
        {#each messages as message, i}
          {@const isLastMessage = i === messages.length - 1}
          {@const isStreamingMessage = isLastMessage && $ai.isStreaming && message.role === 'assistant'}

          {#if message.role === 'user'}
            <!-- User message -->
            <div class="flex justify-end">
              <div class="max-w-[85%] rounded-lg px-3 py-2 text-sm bg-muted text-foreground">
                <p class="whitespace-pre-wrap">{message.content}</p>
              </div>
            </div>
          {:else}
            <!-- Assistant message -->
            <div class="text-sm space-y-2">
              {#if message.thinkingSteps && message.thinkingSteps.length > 0}
                <ThinkingSteps
                  steps={message.thinkingSteps}
                  isStreaming={isStreamingMessage}
                />
              {/if}

              {#if message.toolActions && message.toolActions.length > 0}
                <ToolActionsGroup
                  actions={message.toolActions}
                  onRefreshFiles={() => fileSystem.refresh()}
                />
              {/if}

              {#if message.content}
                <Markdown content={message.content} />
              {/if}

              {#if isStreamingMessage && !message.content && (!message.toolActions || message.toolActions.length === 0)}
                <span class="text-muted-foreground italic animate-pulse">Thinking...</span>
              {/if}
            </div>
          {/if}
        {/each}

        {#if $ai.isStreaming}
          {@const lastMessage = messages[messages.length - 1]}
          {#if !lastMessage || lastMessage.role !== 'assistant'}
            <!-- Show loading dots only if there's no assistant message yet -->
            <div class="text-sm">
              <span class="inline-flex gap-1">
                <span class="w-2 h-2 bg-foreground/50 rounded-full animate-bounce" style="animation-delay: 0ms"></span>
                <span class="w-2 h-2 bg-foreground/50 rounded-full animate-bounce" style="animation-delay: 150ms"></span>
                <span class="w-2 h-2 bg-foreground/50 rounded-full animate-bounce" style="animation-delay: 300ms"></span>
              </span>
            </div>
          {/if}
        {/if}
      {/if}

      {#if $ai.error}
        <div class="bg-destructive/10 text-destructive text-sm rounded-lg px-3 py-2">
          {$ai.error}
        </div>
      {/if}
    </div>

    <!-- Input Area -->
    <div class="flex-shrink-0">
      <!-- Context Pills -->
      <div class="px-3 pt-2 pb-1">
        <ContextPills />
      </div>

      <!-- Main Input -->
      <form onsubmit={handleSubmit} class="p-3 relative">
        {#if showContextPicker}
          <ContextPicker
            bind:this={contextPicker}
            query={contextQuery}
            onSelect={handleFileSelect}
            onClose={closeContextPicker}
          />
        {/if}

        <div class="rounded-lg bg-muted/30 border border-border focus-within:ring-2 focus-within:ring-primary/50">
          <!-- Textarea -->
          <textarea
            bind:this={textareaElement}
            bind:value={inputValue}
            oninput={handleInput}
            onkeydown={handleInputKeyDown}
            placeholder={hasActiveDocument ? 'Ask about your document...' : 'Ask anything...'}
            rows={1}
            disabled={$ai.isStreaming}
            class="w-full resize-none px-3 py-2 text-sm bg-transparent placeholder:text-muted-foreground focus:outline-none min-h-[40px] max-h-[120px] disabled:opacity-50 overflow-y-auto"
          ></textarea>

          <!-- Bottom Toolbar -->
          <div class="flex items-center justify-between px-2 py-1.5 border-t border-border/50">
            <!-- Left side - Model selector & Agent toggle -->
            <div class="flex items-center gap-1">
              <!-- Model Selector -->
              <div class="relative">
                <button
                  type="button"
                  onclick={(e) => { e.stopPropagation(); showModelSelector = !showModelSelector; }}
                  class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-muted transition-colors"
                >
                  <span>{currentModelName}</span>
                  <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="6 9 12 15 18 9"/>
                  </svg>
                </button>
                {#if showModelSelector}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <div
                    class="absolute left-0 bottom-full mb-1 w-48 bg-popover border border-border rounded-md shadow-lg py-1 z-50"
                    onclick={(e) => e.stopPropagation()}
                  >
                    {#each Object.entries(models) as [provider, providerModels]}
                      {#if providerModels.length > 0}
                        <div class="px-2 py-1 text-xs text-muted-foreground uppercase tracking-wide">
                          {provider}
                        </div>
                        {#each providerModels as model (model.id)}
                          <button
                            type="button"
                            onclick={() => selectModel(provider as LLMProvider, model.id)}
                            class="w-full text-left px-3 py-1.5 text-sm hover:bg-accent flex items-center justify-between {$ai.selectedModel === model.id ? 'text-primary' : 'text-foreground'}"
                          >
                            <span>{model.name}</span>
                            {#if $ai.selectedModel === model.id}
                              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-primary">
                                <polyline points="20 6 9 17 4 12"/>
                              </svg>
                            {/if}
                          </button>
                        {/each}
                      {/if}
                    {/each}
                    {#if modelsLoading}
                      <div class="px-3 py-2 text-xs text-muted-foreground">
                        Loading models...
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>

              <!-- Agent Toggle -->
              <button
                type="button"
                onclick={toggleAgentMode}
                class="flex items-center gap-1 text-xs px-2 py-1 rounded transition-colors
                  {$ai.agentEnabled
                    ? 'bg-primary/20 text-primary'
                    : 'text-muted-foreground hover:text-foreground hover:bg-muted'}"
                title={$ai.agentEnabled ? 'Agent mode enabled' : 'Enable agent mode'}
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M12 8V4H8"/>
                  <rect x="8" y="8" width="8" height="8" rx="2"/>
                  <path d="M12 20v-4"/>
                  <path d="M20 12h-4"/>
                  <path d="M4 12h4"/>
                </svg>
                <span>Agent</span>
              </button>
            </div>

            <!-- Right side - Actions -->
            <div class="flex items-center gap-1">
              <!-- @ Context Button -->
              <button
                type="button"
                onclick={handleAtButtonClick}
                class="p-1.5 rounded transition-colors {showContextPicker ? 'text-primary bg-primary/10' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}"
                title="Add file context (@)"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="12" cy="12" r="4"/>
                  <path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.92 7.94"/>
                </svg>
              </button>

              <!-- Send/Cancel Button -->
              {#if $ai.isStreaming}
                <button
                  type="button"
                  onclick={handleCancel}
                  class="p-1.5 rounded text-destructive hover:bg-destructive/10 transition-colors"
                  title="Cancel"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
                  </svg>
                </button>
              {:else}
                <button
                  type="submit"
                  disabled={!inputValue.trim()}
                  class="p-1.5 rounded text-muted-foreground hover:text-foreground hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                  title="Send message"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="22" y1="2" x2="11" y2="13"/>
                    <polygon points="22 2 15 22 11 13 2 9 22 2"/>
                  </svg>
                </button>
              {/if}
            </div>
          </div>
        </div>
      </form>
    </div>
  </div>
{/if}
