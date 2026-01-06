// LLM Integration Types for Midlight

// ============================================================================
// Message Types
// ============================================================================

export type MessageRole = 'system' | 'user' | 'assistant' | 'tool';

export interface ChatMessage {
  role: MessageRole;
  content: string;
  name?: string; // For tool messages
  toolCallId?: string; // For tool result messages
  toolCalls?: ToolCall[]; // For assistant messages that made tool calls
}

export interface ToolResultMessage extends ChatMessage {
  role: 'tool';
  toolCallId: string;
  content: string; // JSON stringified result
}

// ============================================================================
// Provider & Model Types
// ============================================================================

export type LLMProvider = 'openai' | 'anthropic' | 'gemini';

export interface ModelInfo {
  id: string;
  name: string;
  provider: LLMProvider;
  tier: 'free' | 'pro' | 'enterprise';
  contextWindow: number;
  supportsTools: boolean;
  supportsStreaming: boolean;
}

export interface AvailableModels {
  openai: ModelInfo[];
  anthropic: ModelInfo[];
  gemini: ModelInfo[];
}

// ============================================================================
// Chat Options
// ============================================================================

export type RequestType = 'chat' | 'inline_edit' | 'agent';

export interface ChatOptions {
  provider: LLMProvider;
  model: string;
  messages: ChatMessage[];
  temperature?: number;
  maxTokens?: number;
  stream?: boolean;
  requestType?: RequestType;
  webSearchEnabled?: boolean;
}

export interface ChatWithToolsOptions extends ChatOptions {
  tools: ToolDefinition[];
  toolChoice?: 'auto' | 'none' | { type: 'function'; function: { name: string } };
}

// ============================================================================
// Tool Types
// ============================================================================

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: {
    type: 'object';
    properties: Record<string, ToolParameter>;
    required?: string[];
  };
}

export interface ToolParameter {
  type: 'string' | 'number' | 'boolean' | 'array' | 'object';
  description: string;
  enum?: string[];
  items?: ToolParameter;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

// ============================================================================
// Response Types
// ============================================================================

export interface UsageInfo {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

export interface ChatResponse {
  id: string;
  content: string;
  finishReason: 'stop' | 'length' | 'tool_calls' | 'content_filter' | 'error';
  usage?: UsageInfo;
  toolCalls?: ToolCall[];
}

// ============================================================================
// Streaming Types
// ============================================================================

export type StreamChunkType = 'content' | 'tool_call' | 'done' | 'error' | 'usage';

export interface StreamChunk {
  type: StreamChunkType;
  content?: string;
  toolCall?: Partial<ToolCall>;
  error?: string;
  usage?: UsageInfo;
  finishReason?: ChatResponse['finishReason'];
}

export type StreamCallback = (chunk: StreamChunk) => void;

// ============================================================================
// Quota & Status Types
// ============================================================================

export interface QuotaInfo {
  tier: 'free' | 'pro' | 'enterprise';
  limit: number | null; // null = unlimited
  used: number;
  remaining: number | null;
  resetsAt?: string; // ISO timestamp
}

export interface LLMStatus {
  available: boolean;
  providers: {
    openai: boolean;
    anthropic: boolean;
    gemini: boolean;
  };
  message?: string;
}

// ============================================================================
// LLM Client Interface
// ============================================================================

export interface LLMClient {
  /**
   * Send a chat message and receive a complete response
   */
  chat(options: ChatOptions): Promise<ChatResponse>;

  /**
   * Send a chat message and receive streaming response chunks
   */
  chatStream(options: ChatOptions, onChunk: StreamCallback): Promise<ChatResponse>;

  /**
   * Send a chat message with tool definitions for function calling
   */
  chatWithTools(options: ChatWithToolsOptions): Promise<ChatResponse>;

  /**
   * Stream a chat message with tool definitions
   */
  chatWithToolsStream(options: ChatWithToolsOptions, onChunk: StreamCallback): Promise<ChatResponse>;

  /**
   * Get available models for all providers
   */
  getModels(): Promise<AvailableModels>;

  /**
   * Get current quota information
   */
  getQuota(): Promise<QuotaInfo>;

  /**
   * Get LLM service status
   */
  getStatus(): Promise<LLMStatus>;

  /**
   * Cancel an ongoing stream
   */
  cancelStream(streamId: string): void;
}

// ============================================================================
// Context Types (for @ mentions)
// ============================================================================

export interface ContextItem {
  type: 'file' | 'selection' | 'current_document';
  path?: string;
  name: string;
  content: string;
  tokenCount?: number;
}

export interface ContextSettings {
  includeCurrentDocument: boolean;
  includeSelection: boolean;
  maxContextTokens: number;
}

// ============================================================================
// Error Types
// ============================================================================

export class LLMError extends Error {
  constructor(
    message: string,
    public code: LLMErrorCode,
    public details?: Record<string, unknown>
  ) {
    super(message);
    this.name = 'LLMError';
  }
}

export type LLMErrorCode =
  | 'AUTH_REQUIRED'
  | 'AUTH_EXPIRED'
  | 'QUOTA_EXCEEDED'
  | 'RATE_LIMITED'
  | 'PROVIDER_ERROR'
  | 'NETWORK_ERROR'
  | 'INVALID_REQUEST'
  | 'CONTENT_FILTERED'
  | 'STREAM_CANCELLED'
  | 'UNKNOWN';
