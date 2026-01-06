// WebLLMClient - Fetch/SSE-based LLM client for web and as reference implementation

import type {
  LLMClient,
  ChatOptions,
  ChatWithToolsOptions,
  ChatResponse,
  StreamCallback,
  StreamChunk,
  AvailableModels,
  QuotaInfo,
  LLMStatus,
  UsageInfo,
  ToolCall,
} from './types';
import { LLMError } from './types';

export interface WebLLMClientConfig {
  baseUrl: string;
  getAuthToken: () => Promise<string | null>;
}

/**
 * Web-based LLM client using fetch for requests and SSE for streaming.
 * This implementation works in browsers and can be adapted for other platforms.
 */
export class WebLLMClient implements LLMClient {
  private baseUrl: string;
  private getAuthToken: () => Promise<string | null>;
  private activeStreams: Map<string, AbortController> = new Map();

  constructor(config: WebLLMClientConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, ''); // Remove trailing slash
    this.getAuthToken = config.getAuthToken;
  }

  /**
   * Get authorization headers for API requests
   */
  private async getHeaders(): Promise<Headers> {
    const headers = new Headers({
      'Content-Type': 'application/json',
    });

    const token = await this.getAuthToken();
    if (token) {
      headers.set('Authorization', `Bearer ${token}`);
    }

    return headers;
  }

  /**
   * Handle API error responses
   */
  private async handleErrorResponse(response: Response): Promise<never> {
    let errorData: { code?: string; message?: string; details?: Record<string, unknown> } = {};

    try {
      errorData = await response.json();
    } catch {
      // Response body may not be JSON
    }

    const message = errorData.message || `HTTP ${response.status}: ${response.statusText}`;

    switch (response.status) {
      case 401:
        throw new LLMError(message, 'AUTH_REQUIRED', errorData.details);
      case 403:
        if (errorData.code === 'QUOTA_EXCEEDED') {
          throw new LLMError(message, 'QUOTA_EXCEEDED', errorData.details);
        }
        throw new LLMError(message, 'AUTH_EXPIRED', errorData.details);
      case 429:
        throw new LLMError(message, 'RATE_LIMITED', errorData.details);
      case 400:
        throw new LLMError(message, 'INVALID_REQUEST', errorData.details);
      case 451:
        throw new LLMError(message, 'CONTENT_FILTERED', errorData.details);
      default:
        if (response.status >= 500) {
          throw new LLMError(message, 'PROVIDER_ERROR', errorData.details);
        }
        throw new LLMError(message, 'UNKNOWN', errorData.details);
    }
  }

  /**
   * Send a chat message and receive a complete response
   */
  async chat(options: ChatOptions): Promise<ChatResponse> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/llm/chat`, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        provider: options.provider,
        model: options.model,
        messages: options.messages,
        temperature: options.temperature,
        maxTokens: options.maxTokens,
        requestType: options.requestType,
        webSearchEnabled: options.webSearchEnabled,
      }),
    });

    if (!response.ok) {
      await this.handleErrorResponse(response);
    }

    const data = await response.json();

    return {
      id: data.id,
      content: data.content,
      finishReason: data.finishReason,
      usage: data.usage,
      toolCalls: data.toolCalls,
    };
  }

  /**
   * Send a chat message and receive streaming response chunks via SSE
   */
  async chatStream(options: ChatOptions, onChunk: StreamCallback): Promise<ChatResponse> {
    const headers = await this.getHeaders();
    const streamId = crypto.randomUUID();
    const abortController = new AbortController();
    this.activeStreams.set(streamId, abortController);

    try {
      const response = await fetch(`${this.baseUrl}/api/llm/chat`, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          provider: options.provider,
          model: options.model,
          messages: options.messages,
          temperature: options.temperature,
          maxTokens: options.maxTokens,
          stream: true,
          requestType: options.requestType,
          webSearchEnabled: options.webSearchEnabled,
        }),
        signal: abortController.signal,
      });

      if (!response.ok) {
        await this.handleErrorResponse(response);
      }

      return await this.processSSEStream(response, onChunk, streamId);
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        throw new LLMError('Stream cancelled', 'STREAM_CANCELLED');
      }
      throw error;
    } finally {
      this.activeStreams.delete(streamId);
    }
  }

  /**
   * Send a chat message with tool definitions for function calling
   */
  async chatWithTools(options: ChatWithToolsOptions): Promise<ChatResponse> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/llm/chat-with-tools`, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        provider: options.provider,
        model: options.model,
        messages: options.messages,
        temperature: options.temperature,
        maxTokens: options.maxTokens,
        tools: options.tools,
        toolChoice: options.toolChoice,
        requestType: options.requestType,
        webSearchEnabled: options.webSearchEnabled,
      }),
    });

    if (!response.ok) {
      await this.handleErrorResponse(response);
    }

    const data = await response.json();

    return {
      id: data.id,
      content: data.content,
      finishReason: data.finishReason,
      usage: data.usage,
      toolCalls: data.toolCalls,
    };
  }

  /**
   * Stream a chat message with tool definitions
   */
  async chatWithToolsStream(
    options: ChatWithToolsOptions,
    onChunk: StreamCallback
  ): Promise<ChatResponse> {
    const headers = await this.getHeaders();
    const streamId = crypto.randomUUID();
    const abortController = new AbortController();
    this.activeStreams.set(streamId, abortController);

    try {
      const response = await fetch(`${this.baseUrl}/api/llm/chat-with-tools`, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          provider: options.provider,
          model: options.model,
          messages: options.messages,
          temperature: options.temperature,
          maxTokens: options.maxTokens,
          stream: true,
          tools: options.tools,
          toolChoice: options.toolChoice,
          requestType: options.requestType,
          webSearchEnabled: options.webSearchEnabled,
        }),
        signal: abortController.signal,
      });

      if (!response.ok) {
        await this.handleErrorResponse(response);
      }

      return await this.processSSEStream(response, onChunk, streamId);
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        throw new LLMError('Stream cancelled', 'STREAM_CANCELLED');
      }
      throw error;
    } finally {
      this.activeStreams.delete(streamId);
    }
  }

  /**
   * Process SSE stream from response
   */
  private async processSSEStream(
    response: Response,
    onChunk: StreamCallback,
    streamId: string
  ): Promise<ChatResponse> {
    const reader = response.body?.getReader();
    if (!reader) {
      throw new LLMError('No response body', 'PROVIDER_ERROR');
    }

    const decoder = new TextDecoder();
    let buffer = '';
    let accumulatedContent = '';
    let accumulatedToolCalls: Partial<ToolCall>[] = [];
    let finalUsage: UsageInfo | undefined;
    let finishReason: ChatResponse['finishReason'] = 'stop';
    let responseId = streamId;

    try {
      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          break;
        }

        buffer += decoder.decode(value, { stream: true });

        // Process complete SSE events
        const lines = buffer.split('\n');
        buffer = lines.pop() || ''; // Keep incomplete line in buffer

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            const data = line.slice(6);

            if (data === '[DONE]') {
              onChunk({ type: 'done', finishReason });
              continue;
            }

            try {
              const parsed = JSON.parse(data) as StreamChunk & { id?: string };

              if (parsed.id) {
                responseId = parsed.id;
              }

              switch (parsed.type) {
                case 'content':
                  if (parsed.content) {
                    accumulatedContent += parsed.content;
                    onChunk({ type: 'content', content: parsed.content });
                  }
                  break;

                case 'tool_call':
                  if (parsed.toolCall) {
                    // Accumulate tool call parts
                    const existingIndex = accumulatedToolCalls.findIndex(
                      (tc) => tc.id === parsed.toolCall?.id
                    );
                    if (existingIndex >= 0) {
                      // Merge with existing
                      const existing = accumulatedToolCalls[existingIndex];
                      accumulatedToolCalls[existingIndex] = {
                        ...existing,
                        ...parsed.toolCall,
                        arguments: {
                          ...(existing.arguments as Record<string, unknown>),
                          ...(parsed.toolCall.arguments as Record<string, unknown>),
                        },
                      };
                    } else {
                      accumulatedToolCalls.push(parsed.toolCall);
                    }
                    onChunk({ type: 'tool_call', toolCall: parsed.toolCall });
                  }
                  break;

                case 'usage':
                  if (parsed.usage) {
                    finalUsage = parsed.usage;
                    onChunk({ type: 'usage', usage: parsed.usage });
                  }
                  break;

                case 'error':
                  onChunk({ type: 'error', error: parsed.error });
                  throw new LLMError(parsed.error || 'Stream error', 'PROVIDER_ERROR');

                case 'done':
                  if (parsed.finishReason) {
                    finishReason = parsed.finishReason;
                  }
                  onChunk({ type: 'done', finishReason: parsed.finishReason });
                  break;
              }
            } catch (parseError) {
              // Skip malformed JSON
              if (parseError instanceof LLMError) {
                throw parseError;
              }
              console.warn('Failed to parse SSE data:', data);
            }
          }
        }
      }
    } finally {
      reader.releaseLock();
    }

    // Build final response
    const toolCalls: ToolCall[] = accumulatedToolCalls
      .filter((tc): tc is ToolCall => !!tc.id && !!tc.name)
      .map((tc) => ({
        id: tc.id!,
        name: tc.name!,
        arguments: (tc.arguments as Record<string, unknown>) || {},
      }));

    return {
      id: responseId,
      content: accumulatedContent,
      finishReason,
      usage: finalUsage,
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
    };
  }

  /**
   * Get available models for all providers
   */
  async getModels(): Promise<AvailableModels> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/llm/models`, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      await this.handleErrorResponse(response);
    }

    return await response.json();
  }

  /**
   * Get current quota information
   */
  async getQuota(): Promise<QuotaInfo> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/llm/quota`, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      await this.handleErrorResponse(response);
    }

    return await response.json();
  }

  /**
   * Get LLM service status
   */
  async getStatus(): Promise<LLMStatus> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/llm/status`, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      // For status, return unavailable instead of throwing
      return {
        available: false,
        providers: {
          openai: false,
          anthropic: false,
          gemini: false,
        },
        message: `Service unavailable: ${response.status}`,
      };
    }

    return await response.json();
  }

  /**
   * Cancel an ongoing stream
   */
  cancelStream(streamId: string): void {
    const controller = this.activeStreams.get(streamId);
    if (controller) {
      controller.abort();
      this.activeStreams.delete(streamId);
    }
  }

  /**
   * Cancel all active streams
   */
  cancelAllStreams(): void {
    for (const [id, controller] of this.activeStreams) {
      controller.abort();
      this.activeStreams.delete(id);
    }
  }
}

/**
 * Create a WebLLMClient with default configuration
 */
export function createWebLLMClient(
  baseUrl: string = 'https://midlight.ai',
  getAuthToken: () => Promise<string | null>
): WebLLMClient {
  return new WebLLMClient({
    baseUrl,
    getAuthToken,
  });
}
