<script lang="ts">
  import { ai, activeConversation, isStreaming } from '@midlight/stores';

  let inputValue = $state('');
  let activeTab = $state<'chat' | 'versions'>('chat');

  async function sendMessage() {
    if (!inputValue.trim() || $isStreaming) return;

    const message = inputValue.trim();
    inputValue = '';

    // Ensure we have an active conversation
    if (!$ai.activeConversationId) {
      ai.createConversation('New Chat');
    }

    // Add user message
    ai.addMessage({
      role: 'user',
      content: message,
    });

    ai.setIsStreaming(true);

    try {
      // TODO: Integrate with LLM API
      // For now, simulate a response
      await new Promise(resolve => setTimeout(resolve, 1000));

      ai.addMessage({
        role: 'assistant',
        content: 'I\'m the AI assistant. The LLM integration is coming soon! For now, this is a placeholder response.',
      });
    } catch (error) {
      console.error('Failed to send message:', error);
      ai.addMessage({
        role: 'assistant',
        content: 'Sorry, I encountered an error. Please try again.',
      });
    } finally {
      ai.setIsStreaming(false);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  function formatTime(timestamp: string): string {
    return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
</script>

<div class="h-full flex flex-col">
  <!-- Tabs -->
  <div class="h-10 flex border-b border-border">
    <button
      onclick={() => activeTab = 'chat'}
      class="flex-1 text-sm font-medium transition-colors {activeTab === 'chat' ? 'text-foreground border-b-2 border-primary' : 'text-muted-foreground hover:text-foreground'}"
    >
      AI Chat
    </button>
    <button
      onclick={() => activeTab = 'versions'}
      class="flex-1 text-sm font-medium transition-colors {activeTab === 'versions' ? 'text-foreground border-b-2 border-primary' : 'text-muted-foreground hover:text-foreground'}"
    >
      Versions
    </button>
  </div>

  {#if activeTab === 'chat'}
    <!-- Chat Panel -->
    <div class="flex-1 flex flex-col min-h-0">
      <!-- Messages -->
      <div class="flex-1 overflow-auto p-4 space-y-4">
        {#if !$activeConversation || $activeConversation.messages.length === 0}
          <div class="text-center text-muted-foreground py-8">
            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mx-auto mb-4 opacity-50">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
            </svg>
            <p class="text-sm">Start a conversation with AI</p>
            <p class="text-xs mt-1">Ask questions or get help with your document</p>
          </div>
        {:else}
          {#each $activeConversation.messages as message (message.id)}
            <div class="flex gap-3 {message.role === 'user' ? 'flex-row-reverse' : ''}">
              <div class="flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center {message.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted'}">
                {#if message.role === 'user'}
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/>
                    <circle cx="12" cy="7" r="4"/>
                  </svg>
                {:else}
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M12 8V4H8"/>
                    <rect x="8" y="8" width="8" height="8" rx="1"/>
                    <path d="M16 8V4"/>
                    <path d="M8 16v4"/>
                    <path d="M16 16v4"/>
                  </svg>
                {/if}
              </div>
              <div class="flex-1 max-w-[85%]">
                <div class="rounded-lg px-3 py-2 {message.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted'}">
                  <p class="text-sm whitespace-pre-wrap">{message.content}</p>
                </div>
                <span class="text-xs text-muted-foreground mt-1 block {message.role === 'user' ? 'text-right' : ''}">
                  {formatTime(message.timestamp)}
                </span>
              </div>
            </div>
          {/each}

          {#if $isStreaming}
            <div class="flex gap-3">
              <div class="flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center bg-muted">
                <div class="flex gap-1">
                  <span class="w-1.5 h-1.5 bg-foreground rounded-full animate-bounce" style="animation-delay: 0ms"></span>
                  <span class="w-1.5 h-1.5 bg-foreground rounded-full animate-bounce" style="animation-delay: 150ms"></span>
                  <span class="w-1.5 h-1.5 bg-foreground rounded-full animate-bounce" style="animation-delay: 300ms"></span>
                </div>
              </div>
            </div>
          {/if}
        {/if}
      </div>

      <!-- Input -->
      <div class="p-3 border-t border-border">
        <div class="flex gap-2">
          <textarea
            bind:value={inputValue}
            onkeydown={handleKeydown}
            placeholder="Ask AI anything..."
            rows="1"
            class="flex-1 resize-none px-3 py-2 text-sm bg-background border border-input rounded-lg focus:outline-none focus:ring-2 focus:ring-ring"
            disabled={$isStreaming}
          ></textarea>
          <button
            onclick={sendMessage}
            disabled={!inputValue.trim() || $isStreaming}
            class="px-3 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="22" y1="2" x2="11" y2="13"/>
              <polygon points="22 2 15 22 11 13 2 9 22 2"/>
            </svg>
          </button>
        </div>
      </div>
    </div>
  {:else}
    <!-- Versions Panel (placeholder) -->
    <div class="flex-1 flex items-center justify-center text-muted-foreground p-4">
      <div class="text-center">
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mx-auto mb-4 opacity-50">
          <circle cx="12" cy="12" r="10"/>
          <polyline points="12 6 12 12 16 14"/>
        </svg>
        <p class="text-sm">Version history</p>
        <p class="text-xs mt-1">Coming soon</p>
      </div>
    </div>
  {/if}
</div>
