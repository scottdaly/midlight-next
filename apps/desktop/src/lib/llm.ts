// TauriLLMClient - LLM client that uses Tauri commands

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
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
  LLMErrorCode,
} from '@midlight/core';
import { LLMError } from '@midlight/core';

// ============================================================================
// Event Types (matching Rust structs)
// ============================================================================

interface StreamEvent {
  streamId: string;
  chunk: StreamChunk;
}

interface StreamCompleteEvent {
  streamId: string;
  response: ChatResponse;
}

interface StreamErrorEvent {
  streamId: string;
  error: {
    code: string;
    message: string;
    details?: Record<string, unknown>;
  };
}

// ============================================================================
// TauriLLMClient
// ============================================================================

export interface TauriLLMClientConfig {
  getAuthToken: () => Promise<string | null>;
}

/**
 * LLM client that uses Tauri commands for desktop app.
 * Communicates with the Rust backend which handles HTTP requests.
 */
export class TauriLLMClient implements LLMClient {
  private getAuthToken: () => Promise<string | null>;
  private activeStreams: Map<string, { unlisten: UnlistenFn | null; cancelled: boolean }> =
    new Map();

  constructor(config: TauriLLMClientConfig) {
    this.getAuthToken = config.getAuthToken;
  }

  /**
   * Send a chat message and receive a complete response
   */
  async chat(options: ChatOptions): Promise<ChatResponse> {
    const authToken = await this.getAuthToken();

    const response = await invoke<ChatResponse>('llm_chat', {
      options: {
        provider: options.provider,
        model: options.model,
        messages: options.messages,
        temperature: options.temperature,
        maxTokens: options.maxTokens,
        requestType: options.requestType,
        webSearchEnabled: options.webSearchEnabled,
      },
      authToken,
    });

    return response;
  }

  /**
   * Send a chat message and receive streaming response chunks
   */
  async chatStream(options: ChatOptions, onChunk: StreamCallback): Promise<ChatResponse> {
    const authToken = await this.getAuthToken();
    const streamId = crypto.randomUUID();

    // Track this stream
    this.activeStreams.set(streamId, { unlisten: null, cancelled: false });

    return new Promise(async (resolve, reject) => {
      // Listen for stream chunks
      const unlistenChunk = await listen<StreamEvent>('llm:stream', (event) => {
        if (event.payload.streamId === streamId) {
          onChunk(event.payload.chunk);
        }
      });

      // Listen for stream completion
      const unlistenComplete = await listen<StreamCompleteEvent>('llm:stream:complete', (event) => {
        if (event.payload.streamId === streamId) {
          cleanup();
          resolve(event.payload.response);
        }
      });

      // Listen for stream errors
      const unlistenError = await listen<StreamErrorEvent>('llm:stream:error', (event) => {
        if (event.payload.streamId === streamId) {
          cleanup();
          const error = event.payload.error;
          reject(
            new LLMError(error.message, error.code as LLMErrorCode, error.details)
          );
        }
      });

      const cleanup = () => {
        unlistenChunk();
        unlistenComplete();
        unlistenError();
        this.activeStreams.delete(streamId);
      };

      // Store unlisten functions
      const streamInfo = this.activeStreams.get(streamId);
      if (streamInfo) {
        streamInfo.unlisten = () => {
          unlistenChunk();
          unlistenComplete();
          unlistenError();
        };

        // Check if cancelled while setting up
        if (streamInfo.cancelled) {
          cleanup();
          reject(new LLMError('Stream cancelled', 'STREAM_CANCELLED'));
          return;
        }
      }

      // Start the stream
      // Note: Rust uses #[serde(flatten)] so base fields should be at top level
      try {
        await invoke('llm_chat_stream', {
          options: {
            provider: options.provider,
            model: options.model,
            messages: options.messages,
            temperature: options.temperature,
            maxTokens: options.maxTokens,
            requestType: options.requestType,
            webSearchEnabled: options.webSearchEnabled,
            streamId,
          },
          authToken,
        });
      } catch (error) {
        cleanup();
        if (error instanceof Error) {
          reject(new LLMError(error.message, 'UNKNOWN'));
        } else {
          reject(new LLMError(String(error), 'UNKNOWN'));
        }
      }
    });
  }

  /**
   * Send a chat message with tool definitions for function calling
   */
  async chatWithTools(options: ChatWithToolsOptions): Promise<ChatResponse> {
    const authToken = await this.getAuthToken();

    // Note: Rust uses #[serde(flatten)] so all fields should be at top level
    const response = await invoke<ChatResponse>('llm_chat_with_tools', {
      options: {
        provider: options.provider,
        model: options.model,
        messages: options.messages,
        temperature: options.temperature,
        maxTokens: options.maxTokens,
        requestType: options.requestType,
        webSearchEnabled: options.webSearchEnabled,
        tools: options.tools,
        toolChoice: options.toolChoice,
      },
      authToken,
    });

    return response;
  }

  /**
   * Stream a chat message with tool definitions
   */
  async chatWithToolsStream(
    options: ChatWithToolsOptions,
    onChunk: StreamCallback
  ): Promise<ChatResponse> {
    const authToken = await this.getAuthToken();
    const streamId = crypto.randomUUID();

    // Track this stream
    this.activeStreams.set(streamId, { unlisten: null, cancelled: false });

    return new Promise(async (resolve, reject) => {
      // Listen for stream chunks
      const unlistenChunk = await listen<StreamEvent>('llm:stream', (event) => {
        if (event.payload.streamId === streamId) {
          onChunk(event.payload.chunk);
        }
      });

      // Listen for stream completion
      const unlistenComplete = await listen<StreamCompleteEvent>('llm:stream:complete', (event) => {
        if (event.payload.streamId === streamId) {
          cleanup();
          resolve(event.payload.response);
        }
      });

      // Listen for stream errors
      const unlistenError = await listen<StreamErrorEvent>('llm:stream:error', (event) => {
        if (event.payload.streamId === streamId) {
          cleanup();
          const error = event.payload.error;
          reject(
            new LLMError(error.message, error.code as LLMErrorCode, error.details)
          );
        }
      });

      const cleanup = () => {
        unlistenChunk();
        unlistenComplete();
        unlistenError();
        this.activeStreams.delete(streamId);
      };

      // Store unlisten functions
      const streamInfo = this.activeStreams.get(streamId);
      if (streamInfo) {
        streamInfo.unlisten = () => {
          unlistenChunk();
          unlistenComplete();
          unlistenError();
        };

        // Check if cancelled while setting up
        if (streamInfo.cancelled) {
          cleanup();
          reject(new LLMError('Stream cancelled', 'STREAM_CANCELLED'));
          return;
        }
      }

      // Start the stream
      // Note: Rust uses #[serde(flatten)] so all fields should be at top level
      try {
        await invoke('llm_chat_with_tools_stream', {
          options: {
            provider: options.provider,
            model: options.model,
            messages: options.messages,
            temperature: options.temperature,
            maxTokens: options.maxTokens,
            requestType: options.requestType,
            webSearchEnabled: options.webSearchEnabled,
            tools: options.tools,
            toolChoice: options.toolChoice,
            streamId,
          },
          authToken,
        });
      } catch (error) {
        cleanup();
        if (error instanceof Error) {
          reject(new LLMError(error.message, 'UNKNOWN'));
        } else {
          reject(new LLMError(String(error), 'UNKNOWN'));
        }
      }
    });
  }

  /**
   * Get available models for all providers
   */
  async getModels(): Promise<AvailableModels> {
    const authToken = await this.getAuthToken();
    return await invoke<AvailableModels>('llm_get_models', { authToken });
  }

  /**
   * Get current quota information
   */
  async getQuota(): Promise<QuotaInfo> {
    const authToken = await this.getAuthToken();
    return await invoke<QuotaInfo>('llm_get_quota', { authToken });
  }

  /**
   * Get LLM service status
   */
  async getStatus(): Promise<LLMStatus> {
    const authToken = await this.getAuthToken();
    return await invoke<LLMStatus>('llm_get_status', { authToken });
  }

  /**
   * Cancel an ongoing stream
   */
  cancelStream(streamId: string): void {
    const streamInfo = this.activeStreams.get(streamId);
    if (streamInfo) {
      streamInfo.cancelled = true;
      if (streamInfo.unlisten) {
        streamInfo.unlisten();
      }
      this.activeStreams.delete(streamId);
    }
  }

  /**
   * Cancel all active streams
   */
  cancelAllStreams(): void {
    for (const [id, streamInfo] of this.activeStreams) {
      streamInfo.cancelled = true;
      if (streamInfo.unlisten) {
        streamInfo.unlisten();
      }
      this.activeStreams.delete(id);
    }
  }
}

/**
 * Create a TauriLLMClient with the provided auth token getter
 */
export function createTauriLLMClient(
  getAuthToken: () => Promise<string | null>
): TauriLLMClient {
  return new TauriLLMClient({ getAuthToken });
}
