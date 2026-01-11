// @midlight/stores/ai - AI chat state management

// Agent timeout constants (matching Electron app)
const AGENT_TOTAL_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes total
const AGENT_LLM_CALL_TIMEOUT_MS = 60 * 1000; // 60 seconds per LLM call

import { writable, derived, get } from 'svelte/store';
import type { Message, Conversation, ToolAction, DocumentChange, ThinkingStep, ThinkingStepIcon } from '@midlight/core/types';
import { generateId } from '@midlight/core/utils';
import type {
  LLMClient,
  ChatMessage,
  StreamChunk,
  LLMProvider,
  RequestType,
  ToolCall,
  ToolResultMessage,
} from '@midlight/core';
import { LLMError, allAgentTools, isDestructiveTool, isModifyingTool } from '@midlight/core';
import { agent } from './agent.js';
import { fileSystem, type StagedEdit } from './fileSystem.js';

export interface ContextItem {
  type: 'file' | 'selection' | 'document';
  path?: string;
  content?: string;
  label: string;
  /** Path to the project root (for cross-project references) */
  projectPath?: string;
  /** Display name of the project (from .project.midlight) */
  projectName?: string;
}

// Context layer types for hierarchical context assembly
export type ContextLayerType = 'global' | 'project' | 'document' | 'mentioned' | 'selection' | 'semantic';

export interface ContextLayer {
  type: ContextLayerType;
  enabled: boolean;
  content: string;
  source: string;
  priority: number;
  tokenEstimate?: number;
}

// Context loaders (injected from platform layer)
export type GlobalContextLoader = () => Promise<string | null>;
export type ProjectContextLoader = (projectPath: string) => Promise<string | null>;

// Context update hook (called after AI responses for potential context updates)
export type ContextUpdateHook = (
  projectPath: string,
  userMessage: string,
  assistantResponse: string
) => Promise<void>;

export interface InlineEditState {
  isActive: boolean;
  position: { x: number; y: number } | null;
  selectedText: string;
  suggestedText: string;
  selectionFrom: number;
  selectionTo: number;
  isGenerating: boolean;
}

export interface AIState {
  conversations: Conversation[];
  activeConversationId: string | null;
  isStreaming: boolean;
  currentStreamId: string | null;
  error: string | null;
  contextItems: ContextItem[];
  selectionContext: string | null;
  currentDocumentContext: string | null;
  selectedProvider: LLMProvider;
  selectedModel: string;
  temperature: number;
  webSearchEnabled: boolean;
  annotationsVisible: boolean;
  agentEnabled: boolean;
  workspaceRoot: string | null;
  inlineEdit: InlineEditState;

  // Context hierarchy (Phase 2)
  freshStartMode: boolean;
  currentProjectPath: string | null;
  contextLayers: ContextLayer[];
  includeGlobalContext: boolean;
}

// Tool executor function type (injected from platform layer)
export type ToolExecutor = (
  workspaceRoot: string,
  toolName: string,
  args: Record<string, unknown>
) => Promise<{ success: boolean; data?: unknown; error?: string }>;

const initialState: AIState = {
  conversations: [],
  activeConversationId: null,
  isStreaming: false,
  currentStreamId: null,
  error: null,
  contextItems: [],
  selectionContext: null,
  currentDocumentContext: null,
  selectedProvider: 'anthropic',
  selectedModel: 'claude-haiku-4-5-20251001',
  temperature: 0.7,
  webSearchEnabled: false,
  annotationsVisible: true,
  agentEnabled: false,
  workspaceRoot: null,
  inlineEdit: {
    isActive: false,
    position: null,
    selectedText: '',
    suggestedText: '',
    selectionFrom: 0,
    selectionTo: 0,
    isGenerating: false,
  },

  // Context hierarchy (Phase 2)
  freshStartMode: false,
  currentProjectPath: null,
  contextLayers: [],
  includeGlobalContext: true,
};

function createAIStore() {
  const { subscribe, set, update } = writable<AIState>(initialState);

  // LLM client instance (set by platform-specific initialization)
  let llmClient: LLMClient | null = null;

  // Tool executor (set by platform-specific initialization)
  let toolExecutor: ToolExecutor | null = null;

  // Callback for when files are changed by agent (receives the changed file path)
  let onFileChange: ((path: string) => void) | null = null;

  // Context loaders (set by platform-specific initialization)
  let globalContextLoader: GlobalContextLoader | null = null;
  let projectContextLoader: ProjectContextLoader | null = null;

  // Context update hook (called after AI responses)
  let contextUpdateHook: ContextUpdateHook | null = null;

  return {
    subscribe,

    /**
     * Sets the LLM client instance
     */
    setLLMClient(client: LLMClient) {
      llmClient = client;
    },

    /**
     * Gets the LLM client instance
     */
    getLLMClient(): LLMClient | null {
      return llmClient;
    },

    /**
     * Sets the tool executor function
     */
    setToolExecutor(executor: ToolExecutor) {
      toolExecutor = executor;
    },

    /**
     * Sets callback for when files are changed by agent
     */
    setOnFileChange(callback: (path: string) => void) {
      onFileChange = callback;
    },

    /**
     * Sets the global context loader (for me.midlight)
     */
    setGlobalContextLoader(loader: GlobalContextLoader) {
      globalContextLoader = loader;
    },

    /**
     * Sets the project context loader (for context.midlight)
     */
    setProjectContextLoader(loader: ProjectContextLoader) {
      projectContextLoader = loader;
    },

    /**
     * Sets the context update hook (called after AI responses)
     */
    setContextUpdateHook(hook: ContextUpdateHook) {
      contextUpdateHook = hook;
    },

    /**
     * Sets the workspace root path
     */
    setWorkspaceRoot(path: string | null) {
      update((s) => ({ ...s, workspaceRoot: path }));
    },

    /**
     * Sets the current project path (relative to workspace)
     */
    setCurrentProjectPath(path: string | null) {
      update((s) => ({ ...s, currentProjectPath: path }));
    },

    /**
     * Enables or disables Fresh Start mode (ignores global and project context)
     */
    setFreshStartMode(enabled: boolean) {
      update((s) => ({ ...s, freshStartMode: enabled }));
    },

    /**
     * Toggles Fresh Start mode
     */
    toggleFreshStartMode() {
      update((s) => ({ ...s, freshStartMode: !s.freshStartMode }));
    },

    /**
     * Sets whether to include global context (me.midlight)
     */
    setIncludeGlobalContext(enabled: boolean) {
      update((s) => ({ ...s, includeGlobalContext: enabled }));
    },

    /**
     * Detects if a message contains fresh start intent
     */
    detectFreshStartIntent(message: string): boolean {
      const triggers = [
        /ignore (previous |prior )?context/i,
        /fresh (start|perspective)/i,
        /without (the )?(history|context)/i,
        /start fresh/i,
        /forget (everything|what you know)/i,
        /clean slate/i,
      ];
      return triggers.some((t) => t.test(message));
    },

    /**
     * Assembles context layers for AI request
     * Returns layers in priority order (global -> project -> document -> mentions -> selection)
     */
    async assembleContextLayers(): Promise<ContextLayer[]> {
      const state = get({ subscribe });
      const layers: ContextLayer[] = [];

      // Skip global and project context if in Fresh Start mode
      const skipPersistentContext = state.freshStartMode;

      // Layer 1: Global context (me.midlight) - priority 1
      if (!skipPersistentContext && state.includeGlobalContext && globalContextLoader) {
        try {
          const globalContent = await globalContextLoader();
          if (globalContent) {
            layers.push({
              type: 'global',
              enabled: true,
              content: globalContent,
              source: 'me.midlight',
              priority: 1,
              tokenEstimate: Math.ceil(globalContent.length / 4),
            });
          }
        } catch (err) {
          console.warn('[AI] Failed to load global context:', err);
        }
      }

      // Layer 2: Project context (context.midlight) - priority 2
      if (!skipPersistentContext && state.currentProjectPath && projectContextLoader) {
        try {
          const projectContent = await projectContextLoader(state.currentProjectPath);
          if (projectContent) {
            layers.push({
              type: 'project',
              enabled: true,
              content: projectContent,
              source: `${state.currentProjectPath}/context.midlight`,
              priority: 2,
              tokenEstimate: Math.ceil(projectContent.length / 4),
            });
          }
        } catch (err) {
          console.warn('[AI] Failed to load project context:', err);
        }
      }

      // Layer 3: Current document - priority 3
      if (state.currentDocumentContext) {
        layers.push({
          type: 'document',
          enabled: true,
          content: state.currentDocumentContext,
          source: 'current document',
          priority: 3,
          tokenEstimate: Math.ceil(state.currentDocumentContext.length / 4),
        });
      }

      // Layer 4: @-mentioned files - priority 4
      for (const item of state.contextItems) {
        if (item.content) {
          layers.push({
            type: 'mentioned',
            enabled: true,
            content: item.content,
            source: item.path || item.label,
            priority: 4,
            tokenEstimate: Math.ceil(item.content.length / 4),
          });
        }
      }

      // Layer 5: Selection - priority 5
      if (state.selectionContext) {
        layers.push({
          type: 'selection',
          enabled: true,
          content: state.selectionContext,
          source: 'selection',
          priority: 5,
          tokenEstimate: Math.ceil(state.selectionContext.length / 4),
        });
      }

      // Update state with assembled layers
      update((s) => ({ ...s, contextLayers: layers }));

      return layers.sort((a, b) => a.priority - b.priority);
    },

    /**
     * Builds the context system message from assembled layers
     */
    buildContextSystemMessage(layers: ContextLayer[]): string {
      if (layers.length === 0) return '';

      let contextContent = '';

      for (const layer of layers) {
        if (!layer.enabled) continue;

        switch (layer.type) {
          case 'global':
            contextContent += `## About the User (from me.midlight)\n${layer.content}\n\n`;
            break;
          case 'project':
            contextContent += `## Project Context\n${layer.content}\n\n`;
            break;
          case 'document':
            contextContent += `## Current Document\n${layer.content}\n\n`;
            break;
          case 'mentioned':
            contextContent += `## Referenced: ${layer.source}\n${layer.content}\n\n`;
            break;
          case 'selection':
            contextContent += `## Selected Text\n${layer.content}\n\n`;
            break;
        }
      }

      return contextContent.trim();
    },

    /**
     * Enables or disables agent mode
     */
    setAgentEnabled(enabled: boolean) {
      update((s) => ({ ...s, agentEnabled: enabled }));
    },

    /**
     * Creates a new conversation
     */
    createConversation(title = 'New Chat'): string {
      const id = generateId();
      const now = new Date().toISOString();

      update((s) => ({
        ...s,
        conversations: [
          ...s.conversations,
          {
            id,
            title,
            messages: [],
            createdAt: now,
            updatedAt: now,
          },
        ],
        activeConversationId: id,
      }));

      return id;
    },

    /**
     * Sets the active conversation
     */
    setActiveConversation(id: string | null) {
      update((s) => ({ ...s, activeConversationId: id }));
    },

    /**
     * Adds a message to the active conversation
     */
    addMessage(message: Omit<Message, 'id' | 'timestamp'>) {
      const id = generateId();
      const timestamp = new Date().toISOString();

      update((s) => {
        if (!s.activeConversationId) return s;

        return {
          ...s,
          conversations: s.conversations.map((c) =>
            c.id === s.activeConversationId
              ? {
                  ...c,
                  messages: [...c.messages, { ...message, id, timestamp }],
                  updatedAt: timestamp,
                }
              : c
          ),
        };
      });

      return id;
    },

    /**
     * Updates a message in the active conversation
     */
    updateMessage(messageId: string, updates: Partial<Message>) {
      update((s) => ({
        ...s,
        conversations: s.conversations.map((c) =>
          c.id === s.activeConversationId
            ? {
                ...c,
                messages: c.messages.map((m) =>
                  m.id === messageId ? { ...m, ...updates } : m
                ),
              }
            : c
        ),
      }));
    },

    /**
     * Sends a message to the LLM and streams the response
     * @param userContent The user's message content
     * @param requestType The type of request (chat, inline_edit, agent)
     * @returns The assistant message ID
     */
    async sendMessage(
      userContent: string,
      requestType: RequestType = 'chat'
    ): Promise<string | null> {
      if (!llmClient) {
        update((s) => ({ ...s, error: 'LLM client not initialized' }));
        return null;
      }

      const state = get({ subscribe });

      // Ensure we have an active conversation
      let conversationId = state.activeConversationId;
      if (!conversationId) {
        conversationId = this.createConversation();
      }

      // Clear any previous error
      update((s) => ({ ...s, error: null }));

      // Add user message
      this.addMessage({
        role: 'user',
        content: userContent,
      });

      // Create placeholder assistant message
      const assistantMessageId = this.addMessage({
        role: 'assistant',
        content: '',
      });

      // Build message history for API
      const updatedState = get({ subscribe });
      const conversation = updatedState.conversations.find(
        (c) => c.id === conversationId
      );
      if (!conversation) {
        return null;
      }

      // Convert to ChatMessage format
      const chatMessages: ChatMessage[] = [];

      // Add context items as system message if present
      if (
        updatedState.contextItems.length > 0 ||
        updatedState.selectionContext ||
        updatedState.currentDocumentContext
      ) {
        let contextContent = '';

        if (updatedState.currentDocumentContext) {
          contextContent += `Current document:\n${updatedState.currentDocumentContext}\n\n`;
        }

        if (updatedState.selectionContext) {
          contextContent += `Selected text:\n${updatedState.selectionContext}\n\n`;
        }

        for (const item of updatedState.contextItems) {
          if (item.content) {
            contextContent += `${item.label}:\n${item.content}\n\n`;
          }
        }

        if (contextContent) {
          chatMessages.push({
            role: 'system',
            content: `The user has provided the following context:\n\n${contextContent}`,
          });
        }
      }

      // Add conversation messages (excluding the empty assistant message)
      for (const msg of conversation.messages) {
        if (msg.id === assistantMessageId) continue;
        chatMessages.push({
          role: msg.role as 'user' | 'assistant' | 'system',
          content: msg.content,
        });
      }

      // Set streaming state
      const streamId = generateId();
      update((s) => ({
        ...s,
        isStreaming: true,
        currentStreamId: streamId,
      }));

      let accumulatedContent = '';

      try {
        await llmClient.chatStream(
          {
            provider: updatedState.selectedProvider,
            model: updatedState.selectedModel,
            messages: chatMessages,
            temperature: updatedState.temperature,
            stream: true,
            requestType,
            webSearchEnabled: updatedState.webSearchEnabled,
          },
          (chunk: StreamChunk) => {
            if (chunk.type === 'content' && chunk.content) {
              accumulatedContent += chunk.content;
              this.updateMessage(assistantMessageId!, {
                content: accumulatedContent,
              });
            } else if (chunk.type === 'error') {
              update((s) => ({ ...s, error: chunk.error || 'Unknown error' }));
            }
          }
        );

        // Stream complete
        update((s) => ({
          ...s,
          isStreaming: false,
          currentStreamId: null,
        }));

        // Trigger context update evaluation after successful response
        if (contextUpdateHook && updatedState.currentProjectPath && accumulatedContent) {
          // Fire and forget - don't block the UI
          contextUpdateHook(
            updatedState.currentProjectPath,
            userContent,
            accumulatedContent
          ).catch((err) => console.warn('[AI] Context update hook failed:', err));
        }

        return assistantMessageId;
      } catch (error) {
        update((s) => ({
          ...s,
          isStreaming: false,
          currentStreamId: null,
          error: error instanceof LLMError ? error.message : String(error),
        }));

        // Update the assistant message with error indicator
        this.updateMessage(assistantMessageId!, {
          content: accumulatedContent || 'An error occurred while generating the response.',
        });

        return assistantMessageId;
      }
    },

    /**
     * Cancels the current streaming response
     */
    cancelStream() {
      const state = get({ subscribe });
      if (state.currentStreamId && llmClient) {
        llmClient.cancelStream(state.currentStreamId);
        update((s) => ({
          ...s,
          isStreaming: false,
          currentStreamId: null,
        }));
      }
    },

    /**
     * Sends a message with agent tools enabled (agentic loop)
     * This method handles tool calls and continues the conversation until complete
     */
    async sendMessageWithAgent(userContent: string): Promise<string | null> {
      if (!llmClient) {
        update((s) => ({ ...s, error: 'LLM client not initialized' }));
        return null;
      }

      if (!toolExecutor) {
        update((s) => ({ ...s, error: 'Tool executor not initialized' }));
        return null;
      }

      const state = get({ subscribe });

      if (!state.workspaceRoot) {
        update((s) => ({ ...s, error: 'Workspace root not set' }));
        return null;
      }

      // Ensure we have an active conversation
      let conversationId = state.activeConversationId;
      if (!conversationId) {
        conversationId = this.createConversation();
      }

      // Clear any previous error
      update((s) => ({ ...s, error: null }));

      // Add user message
      this.addMessage({
        role: 'user',
        content: userContent,
      });

      // Create placeholder assistant message
      const assistantMessageId = this.addMessage({
        role: 'assistant',
        content: '',
      });

      // Build initial message history
      const buildMessages = (): ChatMessage[] => {
        const currentState = get({ subscribe });
        const conversation = currentState.conversations.find(
          (c) => c.id === conversationId
        );
        if (!conversation) return [];

        const chatMessages: ChatMessage[] = [];

        // Add context items as system message if present
        if (
          currentState.contextItems.length > 0 ||
          currentState.selectionContext ||
          currentState.currentDocumentContext
        ) {
          let contextContent = '';

          if (currentState.currentDocumentContext) {
            contextContent += `Current document:\n${currentState.currentDocumentContext}\n\n`;
          }

          if (currentState.selectionContext) {
            contextContent += `Selected text:\n${currentState.selectionContext}\n\n`;
          }

          for (const item of currentState.contextItems) {
            if (item.content) {
              contextContent += `${item.label}:\n${item.content}\n\n`;
            }
          }

          const agentInstructions = `You are a helpful AI assistant for Midlight, a document editor. You have access to tools to help manage documents in the user's workspace.

IMPORTANT INSTRUCTIONS:
- When you receive a tool result with "success": true, the operation completed successfully. Do NOT repeat the same operation.
- After completing a task, respond with a brief summary of what you did. Do NOT make additional tool calls unless the user asks for more.
- If a document already exists, do NOT try to create it again.
- If you need to edit an existing document, use edit_document, not create_document.
- Once you have completed the user's request, stop and provide a final response.`;

          if (contextContent) {
            chatMessages.push({
              role: 'system',
              content: `${agentInstructions}\n\nThe user has provided the following context:\n\n${contextContent}`,
            });
          }
        } else {
          chatMessages.push({
            role: 'system',
            content: `You are a helpful AI assistant for Midlight, a document editor. You have access to tools to help manage documents in the user's workspace.

IMPORTANT INSTRUCTIONS:
- When you receive a tool result with "success": true, the operation completed successfully. Do NOT repeat the same operation.
- After completing a task, respond with a brief summary of what you did. Do NOT make additional tool calls unless the user asks for more.
- If a document already exists, do NOT try to create it again.
- If you need to edit an existing document, use edit_document, not create_document.
- Once you have completed the user's request, stop and provide a final response.`,
          });
        }

        // Add conversation messages (excluding the empty assistant message we just created)
        for (const msg of conversation.messages) {
          if (msg.id === assistantMessageId) continue;
          chatMessages.push({
            role: msg.role as 'user' | 'assistant' | 'system',
            content: msg.content,
          });
        }

        return chatMessages;
      };

      // Set streaming state
      const streamId = generateId();
      update((s) => ({
        ...s,
        isStreaming: true,
        currentStreamId: streamId,
      }));

      let accumulatedContent = '';
      const toolActions: ToolAction[] = [];
      const documentChanges: DocumentChange[] = [];
      const thinkingSteps: ThinkingStep[] = [];
      const workspaceRoot = state.workspaceRoot;

      // Helper to update thinkingSteps in the message
      const updateThinkingSteps = () => {
        this.updateMessage(assistantMessageId!, {
          content: accumulatedContent,
          toolActions: toolActions.length > 0 ? [...toolActions] : undefined,
          documentChanges: documentChanges.length > 0 ? [...documentChanges] : undefined,
          thinkingSteps: thinkingSteps.length > 0 ? [...thinkingSteps] : undefined,
        });
      };

      // Helper to add a thinking step
      const addThinkingStep = (step: ThinkingStep) => {
        thinkingSteps.push(step);
        updateThinkingSteps();
      };

      // Helper to mark all active steps as completed
      const completeActiveSteps = () => {
        let changed = false;
        for (const step of thinkingSteps) {
          if (step.status === 'active') {
            step.status = 'completed';
            changed = true;
          }
        }
        if (changed) {
          updateThinkingSteps();
        }
      };

      try {
        // Agent loop - continue until no more tool calls
        const MAX_ITERATIONS = 15;
        let continueLoop = true;
        let messages = buildMessages();
        let iterations = 0;
        const startTime = Date.now();

        while (continueLoop) {
          iterations++;

          // Check total timeout
          if (Date.now() - startTime > AGENT_TOTAL_TIMEOUT_MS) {
            console.warn('[Agent] Total timeout reached, stopping loop');
            this.updateMessage(assistantMessageId!, {
              content: accumulatedContent + '\n\n*Agent execution timed out after 5 minutes. Please continue with a new message if needed.*',
              toolActions: toolActions.length > 0 ? [...toolActions] : undefined,
              documentChanges: documentChanges.length > 0 ? [...documentChanges] : undefined,
              thinkingSteps: thinkingSteps.length > 0 ? [...thinkingSteps] : undefined,
            });
            break;
          }

          // Safety limit to prevent infinite loops
          if (iterations > MAX_ITERATIONS) {
            console.warn('[Agent] Max iterations reached, stopping loop');
            this.updateMessage(assistantMessageId!, {
              content: accumulatedContent + '\n\n*Reached maximum number of actions. Please continue with a new message if needed.*',
              toolActions: toolActions.length > 0 ? [...toolActions] : undefined,
              documentChanges: documentChanges.length > 0 ? [...documentChanges] : undefined,
              thinkingSteps: thinkingSteps.length > 0 ? [...thinkingSteps] : undefined,
            });
            break;
          }

          // Add thinking step for this iteration
          if (iterations === 1) {
            addThinkingStep(createThinkingStep('Analyzing request', 'analyze'));
          } else {
            addThinkingStep(createThinkingStep('Processing results', 'thinking'));
          }

          // Make the API call with tools (non-streaming - backend doesn't support streaming for tools)
          console.log('[Agent] Iteration', iterations, '- Calling chatWithTools with', allAgentTools.length, 'tools');
          console.log('[Agent] Messages being sent:', messages.map(m => ({
            role: m.role,
            content: m.content?.slice(0, 100),
            hasToolCalls: !!(m as any).toolCalls?.length,
            toolCallId: (m as any).toolCallId,
          })));

          // Wrap LLM call with per-call timeout
          const response = await Promise.race([
            llmClient.chatWithTools({
              provider: state.selectedProvider,
              model: state.selectedModel,
              messages,
              temperature: state.temperature,
              requestType: 'agent',
              webSearchEnabled: state.webSearchEnabled,
              tools: allAgentTools,
              toolChoice: 'auto',
            }),
            new Promise<never>((_, reject) =>
              setTimeout(() => reject(new Error('LLM request timed out after 60 seconds')), AGENT_LLM_CALL_TIMEOUT_MS)
            ),
          ]);

          console.log('[Agent] Response:', {
            content: response.content?.slice(0, 100),
            finishReason: response.finishReason,
            toolCallsCount: response.toolCalls?.length || 0,
            toolCalls: response.toolCalls,
          });

          // Update message with any content from the response
          if (response.content) {
            accumulatedContent = response.content;
            this.updateMessage(assistantMessageId!, {
              content: accumulatedContent,
              toolActions: toolActions.length > 0 ? [...toolActions] : undefined,
              documentChanges: documentChanges.length > 0 ? [...documentChanges] : undefined,
              thinkingSteps: thinkingSteps.length > 0 ? [...thinkingSteps] : undefined,
            });
          }

          // Get tool calls directly from response
          const pendingToolCalls = response.toolCalls || [];
          console.log('[Agent] Pending tool calls:', pendingToolCalls.length);

          // Mark "Analyzing" step as complete
          completeActiveSteps();

          // If no tool calls, we're done
          // Note: Different providers use different finish reasons for tool calls:
          // - OpenAI: 'tool_calls'
          // - Anthropic: 'tool_use'
          // So we just check if there are any tool calls rather than checking finishReason
          if (pendingToolCalls.length === 0) {
            continueLoop = false;
            break;
          }

          // Execute each tool call
          const toolResults: ToolResultMessage[] = [];

          for (const toolCall of pendingToolCalls) {
            // Add thinking step for this tool
            const toolStep = toolToThinkingStep(toolCall.name, toolCall.arguments);
            addThinkingStep(toolStep);

            // Create tool action for UI
            const toolAction: ToolAction = {
              id: toolCall.id,
              type: mapToolToActionType(toolCall.name),
              label: getToolLabel(toolCall.name, toolCall.arguments),
              path: toolCall.arguments.path as string | undefined,
              status: 'running',
            };
            toolActions.push(toolAction);

            // Update message with tool action and thinking steps
            updateThinkingSteps();

            // Start execution tracking
            const executionId = agent.startExecution(toolCall, toolAction.label);

            try {
              // Execute the tool
              const result = await toolExecutor(
                workspaceRoot,
                toolCall.name,
                toolCall.arguments
              );

              // Complete execution tracking
              agent.completeExecution(executionId, result);

              // Mark thinking step as completed
              toolStep.status = 'completed';

              // Update tool action status
              toolAction.status = result.success ? 'complete' : 'error';
              toolAction.result = result.data;

              // Track document changes
              if (result.success && isModifyingTool(toolCall.name)) {
                const change: DocumentChange = {
                  type: mapToolToChangeType(toolCall.name),
                  path: (toolCall.arguments.path || toolCall.arguments.oldPath) as string,
                };

                if (toolCall.name === 'move_document') {
                  change.newPath = toolCall.arguments.newPath as string;
                }

                if (toolCall.name === 'edit_document' && result.data) {
                  const editResult = result.data as {
                    changeId?: string;
                    originalContent?: string;
                    newContent?: string;
                    originalTiptapJson?: unknown;
                    stagedTiptapJson?: unknown;
                    requiresAcceptance?: boolean;
                  };
                  change.changeId = editResult.changeId;
                  change.contentBefore = editResult.originalContent;
                  change.contentAfter = editResult.newContent;

                  // Check if this edit requires user acceptance (staged edit)
                  if (editResult.requiresAcceptance && editResult.stagedTiptapJson) {
                    // Stage the edit for visual diff display
                    const stagedEdit: StagedEdit = {
                      changeId: editResult.changeId || generateId(),
                      path: toolCall.arguments.path as string,
                      originalTiptapJson: editResult.originalTiptapJson as StagedEdit['originalTiptapJson'],
                      stagedTiptapJson: editResult.stagedTiptapJson as StagedEdit['stagedTiptapJson'],
                      originalText: editResult.originalContent || '',
                      newText: editResult.newContent || '',
                      description: toolCall.arguments.description as string | undefined,
                      createdAt: new Date().toISOString(),
                      // Context for AI annotations
                      conversationId: conversationId || undefined,
                      messageId: assistantMessageId || undefined,
                    };
                    fileSystem.stageEdit(stagedEdit);
                    // Don't call onFileChange - wait for accept/reject
                  } else {
                    // Legacy behavior: Track pending change for user review
                    if (editResult.changeId) {
                      agent.addPendingChange({
                        changeId: editResult.changeId,
                        path: toolCall.arguments.path as string,
                        originalContent: editResult.originalContent || '',
                        newContent: editResult.newContent || '',
                        description: toolCall.arguments.description as string | undefined,
                        toolExecutionId: executionId,
                      });
                    }
                    // Notify that files have changed (for UI refresh)
                    if (onFileChange) {
                      onFileChange(change.path);
                    }
                  }
                } else {
                  // Non-edit modifying tools: notify file change immediately
                  if (onFileChange) {
                    onFileChange(change.path);
                  }
                }

                documentChanges.push(change);
              }

              // Create tool result message
              toolResults.push({
                role: 'tool',
                toolCallId: toolCall.id,
                name: toolCall.name, // Include tool name for Gemini
                content: JSON.stringify(result),
              });
            } catch (error) {
              // Handle execution error
              agent.completeExecution(executionId, {
                success: false,
                error: String(error),
              });

              // Mark thinking step as completed (even on error)
              toolStep.status = 'completed';
              toolAction.status = 'error';

              toolResults.push({
                role: 'tool',
                toolCallId: toolCall.id,
                name: toolCall.name, // Include tool name for Gemini
                content: JSON.stringify({
                  success: false,
                  error: String(error),
                }),
              });
            }

            // Update message with updated tool actions and thinking steps
            updateThinkingSteps();
          }

          // Add assistant message with tool calls and tool results to history
          messages = buildMessages();

          // Add the assistant's response with content AND tool calls
          // The API requires tool calls to be included in the assistant message
          // so the subsequent tool results can reference them
          if (accumulatedContent || pendingToolCalls.length > 0) {
            messages.push({
              role: 'assistant',
              content: accumulatedContent || '',
              toolCalls: pendingToolCalls.length > 0 ? pendingToolCalls : undefined,
            });
          }

          // Add tool results
          for (const result of toolResults) {
            messages.push(result);
          }

          // Reset accumulated content for next iteration
          accumulatedContent = '';
        }

        // Mark any remaining active steps as completed
        completeActiveSteps();

        // Stream complete
        update((s) => ({
          ...s,
          isStreaming: false,
          currentStreamId: null,
        }));

        // Get the final assistant message content for context update
        const finalState = get({ subscribe });
        const finalConversation = finalState.conversations.find((c) => c.id === conversationId);
        const finalAssistantMessage = finalConversation?.messages.find((m) => m.id === assistantMessageId);

        // Trigger context update evaluation after successful agent response
        if (contextUpdateHook && state.currentProjectPath && finalAssistantMessage?.content) {
          // Fire and forget - don't block the UI
          contextUpdateHook(
            state.currentProjectPath,
            userContent,
            finalAssistantMessage.content
          ).catch((err) => console.warn('[AI] Context update hook failed:', err));
        }

        return assistantMessageId;
      } catch (error) {
        // Mark any active steps as completed
        completeActiveSteps();

        update((s) => ({
          ...s,
          isStreaming: false,
          currentStreamId: null,
          error: error instanceof LLMError ? error.message : String(error),
        }));

        this.updateMessage(assistantMessageId!, {
          content: accumulatedContent || 'An error occurred while generating the response.',
          toolActions: toolActions.length > 0 ? toolActions : undefined,
          thinkingSteps: thinkingSteps.length > 0 ? thinkingSteps : undefined,
        });

        return assistantMessageId;
      }
    },

    /**
     * Sets streaming state
     */
    setIsStreaming(isStreaming: boolean) {
      update((s) => ({ ...s, isStreaming }));
    },

    /**
     * Alias for setIsStreaming for convenience
     */
    setStreaming(isStreaming: boolean) {
      update((s) => ({ ...s, isStreaming }));
    },

    /**
     * Sets error state
     */
    setError(error: string | null) {
      update((s) => ({ ...s, error }));
    },

    /**
     * Clears messages in the active conversation
     */
    clearConversation() {
      update((s) => {
        if (!s.activeConversationId) return s;
        return {
          ...s,
          conversations: s.conversations.map((c) =>
            c.id === s.activeConversationId
              ? { ...c, messages: [], updatedAt: new Date().toISOString() }
              : c
          ),
        };
      });
    },

    /**
     * Sets context items
     */
    setContextItems(items: ContextItem[]) {
      update((s) => ({ ...s, contextItems: items }));
    },

    /**
     * Adds a context item
     */
    addContextItem(item: ContextItem) {
      update((s) => ({
        ...s,
        contextItems: [...s.contextItems, item],
      }));
    },

    /**
     * Removes a context item
     */
    removeContextItem(index: number) {
      update((s) => ({
        ...s,
        contextItems: s.contextItems.filter((_, i) => i !== index),
      }));
    },

    /**
     * Sets selection context
     */
    setSelectionContext(selection: string | null) {
      update((s) => ({ ...s, selectionContext: selection }));
    },

    /**
     * Sets current document context
     */
    setCurrentDocumentContext(content: string | null) {
      update((s) => ({ ...s, currentDocumentContext: content }));
    },

    /**
     * Sets the selected provider
     */
    setProvider(provider: AIState['selectedProvider']) {
      update((s) => ({ ...s, selectedProvider: provider }));
    },

    /**
     * Sets the selected model
     */
    setModel(model: string) {
      update((s) => ({ ...s, selectedModel: model }));
    },

    /**
     * Sets temperature
     */
    setTemperature(temperature: number) {
      update((s) => ({ ...s, temperature }));
    },

    /**
     * Toggles web search
     */
    setWebSearchEnabled(enabled: boolean) {
      update((s) => ({ ...s, webSearchEnabled: enabled }));
    },

    /**
     * Toggles annotations visibility
     */
    setAnnotationsVisible(visible: boolean) {
      update((s) => ({ ...s, annotationsVisible: visible }));
    },

    /**
     * Deletes a conversation
     */
    deleteConversation(id: string) {
      update((s) => ({
        ...s,
        conversations: s.conversations.filter((c) => c.id !== id),
        activeConversationId:
          s.activeConversationId === id ? null : s.activeConversationId,
      }));
    },

    /**
     * Updates conversation title
     */
    updateConversationTitle(id: string, title: string) {
      update((s) => ({
        ...s,
        conversations: s.conversations.map((c) =>
          c.id === id ? { ...c, title } : c
        ),
      }));
    },

    // ========================================================================
    // Inline Edit Methods
    // ========================================================================

    /**
     * Starts inline edit mode with the given selection
     */
    startInlineEdit(
      position: { x: number; y: number },
      selectedText: string,
      selectionFrom: number,
      selectionTo: number
    ) {
      update((s) => ({
        ...s,
        inlineEdit: {
          isActive: true,
          position,
          selectedText,
          suggestedText: '',
          selectionFrom,
          selectionTo,
          isGenerating: false,
        },
      }));
    },

    /**
     * Updates the inline edit suggestion (for streaming)
     */
    setInlineSuggestion(text: string) {
      update((s) => ({
        ...s,
        inlineEdit: {
          ...s.inlineEdit,
          suggestedText: text,
        },
      }));
    },

    /**
     * Sets the inline edit generating state
     */
    setInlineEditGenerating(isGenerating: boolean) {
      update((s) => ({
        ...s,
        inlineEdit: {
          ...s.inlineEdit,
          isGenerating,
        },
      }));
    },

    /**
     * Cancels inline edit mode
     */
    cancelInlineEdit() {
      update((s) => ({
        ...s,
        inlineEdit: {
          isActive: false,
          position: null,
          selectedText: '',
          suggestedText: '',
          selectionFrom: 0,
          selectionTo: 0,
          isGenerating: false,
        },
      }));
    },

    /**
     * Gets the current inline edit state and clears it
     * Returns the suggested text for the editor to apply
     */
    acceptInlineEdit(): string {
      const state = get({ subscribe });
      const suggestedText = state.inlineEdit.suggestedText;

      update((s) => ({
        ...s,
        inlineEdit: {
          isActive: false,
          position: null,
          selectedText: '',
          suggestedText: '',
          selectionFrom: 0,
          selectionTo: 0,
          isGenerating: false,
        },
      }));

      return suggestedText;
    },

    /**
     * Sends an inline edit request to the LLM
     * @param instruction The user's instruction for how to edit the text
     */
    async sendInlineEditRequest(instruction: string): Promise<string | null> {
      if (!llmClient) {
        update((s) => ({ ...s, error: 'LLM client not initialized' }));
        return null;
      }

      const state = get({ subscribe });
      const { selectedText } = state.inlineEdit;

      if (!selectedText) {
        return null;
      }

      // Set generating state
      update((s) => ({
        ...s,
        inlineEdit: { ...s.inlineEdit, isGenerating: true },
        error: null,
      }));

      // Build the prompt for inline editing
      const systemMessage = `You are a helpful writing assistant. The user has selected some text and wants you to edit it according to their instructions.

IMPORTANT: Only output the edited text, nothing else. Do not include any explanation, preamble, or commentary. Just the edited text that should replace the selection.`;

      const userMessage = `Selected text:
"""
${selectedText}
"""

Instruction: ${instruction}

Provide only the edited text:`;

      const messages: ChatMessage[] = [
        { role: 'system', content: systemMessage },
        { role: 'user', content: userMessage },
      ];

      let accumulatedText = '';

      try {
        await llmClient.chatStream(
          {
            provider: state.selectedProvider,
            model: state.selectedModel,
            messages,
            temperature: 0.7,
            stream: true,
            requestType: 'inline_edit',
          },
          (chunk: StreamChunk) => {
            if (chunk.type === 'content' && chunk.content) {
              accumulatedText += chunk.content;
              this.setInlineSuggestion(accumulatedText);
            } else if (chunk.type === 'error') {
              update((s) => ({ ...s, error: chunk.error || 'Unknown error' }));
            }
          }
        );

        // Done generating
        update((s) => ({
          ...s,
          inlineEdit: { ...s.inlineEdit, isGenerating: false },
        }));

        return accumulatedText;
      } catch (error) {
        update((s) => ({
          ...s,
          error: error instanceof LLMError ? error.message : String(error),
          inlineEdit: { ...s.inlineEdit, isGenerating: false },
        }));
        return null;
      }
    },

    /**
     * Resets the store
     */
    reset() {
      set(initialState);
    },
  };
}

// ============================================================================
// Helper Functions for Agent Loop
// ============================================================================

/**
 * Maps a tool name to a ToolAction type
 */
function mapToolToActionType(toolName: string): ToolAction['type'] {
  const mapping: Record<string, ToolAction['type']> = {
    list_documents: 'list',
    read_document: 'read',
    create_document: 'create',
    edit_document: 'edit',
    move_document: 'move',
    delete_document: 'delete',
    search_documents: 'search',
  };
  return mapping[toolName] || 'read';
}

/**
 * Maps a tool name to a DocumentChange type
 */
function mapToolToChangeType(toolName: string): DocumentChange['type'] {
  const mapping: Record<string, DocumentChange['type']> = {
    create_document: 'create',
    edit_document: 'edit',
    move_document: 'move',
    delete_document: 'delete',
  };
  return mapping[toolName] || 'edit';
}

/**
 * Generates a human-readable label for a tool action
 */
function getToolLabel(toolName: string, args: Record<string, unknown>): string {
  switch (toolName) {
    case 'list_documents':
      return `Listing documents in ${args.path || '/'}`;
    case 'read_document':
      return `Reading ${args.path}`;
    case 'create_document':
      return `Creating ${args.path}`;
    case 'edit_document':
      return `Editing ${args.path}`;
    case 'move_document':
      return `Moving ${args.oldPath} to ${args.newPath}`;
    case 'delete_document':
      return `Deleting ${args.path}`;
    case 'search_documents':
      return `Searching for "${args.query}"`;
    default:
      return toolName;
  }
}

// ============================================================================
// Helper Functions for ThinkingSteps
// ============================================================================

/**
 * Creates a new thinking step
 */
function createThinkingStep(label: string, icon: ThinkingStepIcon): ThinkingStep {
  return {
    id: `step-${Date.now()}-${generateId().slice(0, 6)}`,
    label,
    icon,
    status: 'active',
    timestamp: Date.now(),
  };
}

/**
 * Maps a tool name to a ThinkingStep with appropriate label and icon
 */
function toolToThinkingStep(toolName: string, args: Record<string, unknown>): ThinkingStep {
  const getFileName = (p: unknown): string => {
    if (typeof p !== 'string') return 'document';
    return p.split('/').pop() || 'document';
  };

  switch (toolName) {
    case 'read_document':
      return createThinkingStep(`Reading ${getFileName(args.path)}`, 'read');
    case 'list_documents':
      return createThinkingStep(`Browsing ${(args.path as string) || 'documents'}`, 'search');
    case 'search_documents':
      return createThinkingStep(`Searching for "${args.query}"`, 'search');
    case 'create_document':
      return createThinkingStep(`Creating ${getFileName(args.path)}`, 'create');
    case 'edit_document':
      return createThinkingStep(`Editing ${getFileName(args.path)}`, 'edit');
    case 'move_document':
      return createThinkingStep(`Moving ${getFileName(args.oldPath)}`, 'move');
    case 'delete_document':
      return createThinkingStep(`Deleting ${getFileName(args.path)}`, 'delete');
    case 'create_folder':
      return createThinkingStep('Creating folder', 'folder');
    default:
      return createThinkingStep(`Using ${toolName}`, 'thinking');
  }
}

export const ai = createAIStore();

// Derived stores
export const isStreaming = derived(ai, ($ai) => $ai.isStreaming);

export const agentEnabled = derived(ai, ($ai) => $ai.agentEnabled);

export const activeConversation = derived(ai, ($ai) =>
  $ai.conversations.find((c) => c.id === $ai.activeConversationId)
);

export const inlineEditState = derived(ai, ($ai) => $ai.inlineEdit);

export const isInlineEditActive = derived(ai, ($ai) => $ai.inlineEdit.isActive);

// Context hierarchy derived stores (Phase 2)
export const freshStartMode = derived(ai, ($ai) => $ai.freshStartMode);

export const contextLayers = derived(ai, ($ai) => $ai.contextLayers);

export const currentProjectPath = derived(ai, ($ai) => $ai.currentProjectPath);

export const includeGlobalContext = derived(ai, ($ai) => $ai.includeGlobalContext);

export const totalContextTokens = derived(ai, ($ai) =>
  $ai.contextLayers.reduce((sum, layer) => sum + (layer.tokenEstimate || 0), 0)
);
