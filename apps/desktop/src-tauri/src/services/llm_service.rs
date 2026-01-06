// LLM Service - HTTP client for LLM API communication

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

const DEFAULT_BASE_URL: &str = "https://midlight.ai";

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolParameter {
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<ToolParameter>>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolParameters {
    #[serde(rename = "type")]
    pub param_type: String,
    pub properties: serde_json::Map<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: ToolParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub provider: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatWithToolsRequest {
    #[serde(flatten)]
    pub base: ChatRequest,
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub id: String,
    pub content: String,
    pub finish_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// StreamChunk is the normalized chunk format sent to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunk {
    #[serde(rename = "type")]
    pub chunk_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// BackendSSEChunk is the raw format from the backend API
/// Backend sends: { "content": "..." } or { "done": true, "usage": {...} } or { "error": "..." }
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendSSEChunk {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    done: Option<bool>,
    #[serde(default)]
    usage: Option<UsageInfo>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModels {
    pub openai: Vec<ModelInfo>,
    pub anthropic: Vec<ModelInfo>,
    pub gemini: Vec<ModelInfo>,
}

// API response wrapper for models endpoint
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelsResponse {
    models: AvailableModels,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaInfo {
    pub tier: String,
    pub limit: Option<u32>,
    pub used: u32,
    pub remaining: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub openai: bool,
    pub anthropic: bool,
    pub gemini: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMStatus {
    pub available: bool,
    pub providers: ProviderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl std::fmt::Display for LLMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LLMError {}

// ============================================================================
// LLM Service
// ============================================================================

pub struct LLMService {
    client: Client,
    base_url: String,
}

impl LLMService {
    pub fn new(base_url: Option<String>) -> Self {
        // Build default headers for all requests
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            reqwest::header::HeaderName::from_static("x-client-type"),
            reqwest::header::HeaderValue::from_static("desktop"),
        );

        let client = Client::builder()
            .default_headers(default_headers)
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        }
    }

    /// Send a non-streaming chat request
    pub async fn chat(
        &self,
        request: ChatRequest,
        auth_token: Option<&str>,
    ) -> Result<ChatResponse, LLMError> {
        let url = format!("{}/api/llm/chat", self.base_url);

        let mut req = self.client.post(&url).json(&request);

        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| LLMError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })?;

        self.handle_response(response).await
    }

    /// Send a streaming chat request, returning chunks via channel
    pub async fn chat_stream(
        &self,
        request: ChatRequest,
        auth_token: Option<&str>,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse, LLMError> {
        let mut streaming_request = request.clone();
        streaming_request.stream = Some(true);

        let url = format!("{}/api/llm/chat", self.base_url);

        let mut req = self.client.post(&url).json(&streaming_request);

        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| LLMError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.process_sse_stream(response, tx).await
    }

    /// Send a chat request with tools (non-streaming)
    pub async fn chat_with_tools(
        &self,
        request: ChatWithToolsRequest,
        auth_token: Option<&str>,
    ) -> Result<ChatResponse, LLMError> {
        let url = format!("{}/api/llm/chat-with-tools", self.base_url);

        let mut req = self.client.post(&url).json(&request);

        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| LLMError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })?;

        self.handle_response(response).await
    }

    /// Send a streaming chat request with tools
    pub async fn chat_with_tools_stream(
        &self,
        request: ChatWithToolsRequest,
        auth_token: Option<&str>,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse, LLMError> {
        let mut streaming_request = request.clone();
        streaming_request.base.stream = Some(true);

        let url = format!("{}/api/llm/chat-with-tools", self.base_url);

        let mut req = self.client.post(&url).json(&streaming_request);

        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| LLMError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.process_sse_stream(response, tx).await
    }

    /// Get available models
    pub async fn get_models(&self, auth_token: Option<&str>) -> Result<AvailableModels, LLMError> {
        let url = format!("{}/api/llm/models", self.base_url);

        let mut req = self.client.get(&url);

        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| LLMError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let wrapper: ModelsResponse = response.json().await.map_err(|e| LLMError {
            code: "PARSE_ERROR".to_string(),
            message: format!("error decoding response body: {}", e),
            details: None,
        })?;

        Ok(wrapper.models)
    }

    /// Get current quota
    pub async fn get_quota(&self, auth_token: Option<&str>) -> Result<QuotaInfo, LLMError> {
        let url = format!("{}/api/llm/quota", self.base_url);

        let mut req = self.client.get(&url);

        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| LLMError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        response.json().await.map_err(|e| LLMError {
            code: "PARSE_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })
    }

    /// Get LLM service status
    pub async fn get_status(&self, auth_token: Option<&str>) -> Result<LLMStatus, LLMError> {
        let url = format!("{}/api/llm/status", self.base_url);

        let mut req = self.client.get(&url);

        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| {
            // For status, return unavailable instead of error
            warn!("Failed to get LLM status: {}", e);
            LLMError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
                details: None,
            }
        })?;

        if !response.status().is_success() {
            return Ok(LLMStatus {
                available: false,
                providers: ProviderStatus {
                    openai: false,
                    anthropic: false,
                    gemini: false,
                },
                message: Some(format!("Service unavailable: {}", response.status())),
            });
        }

        response.json().await.map_err(|e| LLMError {
            code: "PARSE_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })
    }

    /// Handle a successful response
    async fn handle_response(
        &self,
        response: reqwest::Response,
    ) -> Result<ChatResponse, LLMError> {
        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        response.json().await.map_err(|e| LLMError {
            code: "PARSE_ERROR".to_string(),
            message: e.to_string(),
            details: None,
        })
    }

    /// Parse an error response
    async fn parse_error_response(&self, response: reqwest::Response) -> LLMError {
        let status = response.status();

        let error_body: Option<serde_json::Value> = response.json().await.ok();

        let message = error_body
            .as_ref()
            .and_then(|b| b.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or(&format!("HTTP {}", status))
            .to_string();

        let code = match status.as_u16() {
            401 => "AUTH_REQUIRED",
            403 => {
                if error_body
                    .as_ref()
                    .and_then(|b| b.get("code"))
                    .and_then(|c| c.as_str())
                    == Some("QUOTA_EXCEEDED")
                {
                    "QUOTA_EXCEEDED"
                } else {
                    "AUTH_EXPIRED"
                }
            }
            429 => "RATE_LIMITED",
            400 => "INVALID_REQUEST",
            451 => "CONTENT_FILTERED",
            _ if status.is_server_error() => "PROVIDER_ERROR",
            _ => "UNKNOWN",
        };

        LLMError {
            code: code.to_string(),
            message,
            details: error_body,
        }
    }

    /// Process an SSE stream response
    /// Parses backend format and converts to normalized StreamChunk for frontend
    async fn process_sse_stream(
        &self,
        response: reqwest::Response,
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse, LLMError> {
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut accumulated_content = String::new();
        let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
        let mut final_usage: Option<UsageInfo> = None;
        let mut finish_reason = "stop".to_string();
        let mut response_id = uuid::Uuid::new_v4().to_string();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| LLMError {
                code: "STREAM_ERROR".to_string(),
                message: e.to_string(),
                details: None,
            })?;

            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            // Process complete SSE events
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.starts_with("data: ") {
                    let data = &line[6..];

                    if data == "[DONE]" {
                        let _ = tx
                            .send(StreamChunk {
                                chunk_type: "done".to_string(),
                                content: None,
                                tool_call: None,
                                error: None,
                                usage: final_usage.clone(),
                                finish_reason: Some(finish_reason.clone()),
                                id: None,
                            })
                            .await;
                        continue;
                    }

                    // Parse backend format: { content?, done?, usage?, error? }
                    match serde_json::from_str::<BackendSSEChunk>(data) {
                        Ok(backend_chunk) => {
                            // Convert to normalized StreamChunk format
                            if let Some(ref content) = backend_chunk.content {
                                // Content chunk
                                accumulated_content.push_str(content);
                                let _ = tx
                                    .send(StreamChunk {
                                        chunk_type: "content".to_string(),
                                        content: Some(content.clone()),
                                        tool_call: None,
                                        error: None,
                                        usage: None,
                                        finish_reason: None,
                                        id: None,
                                    })
                                    .await;
                            } else if backend_chunk.done == Some(true) {
                                // Done chunk with usage
                                if let Some(ref usage) = backend_chunk.usage {
                                    final_usage = Some(usage.clone());
                                }
                                let _ = tx
                                    .send(StreamChunk {
                                        chunk_type: "usage".to_string(),
                                        content: None,
                                        tool_call: None,
                                        error: None,
                                        usage: backend_chunk.usage.clone(),
                                        finish_reason: None,
                                        id: None,
                                    })
                                    .await;
                            } else if let Some(ref error) = backend_chunk.error {
                                // Error chunk
                                error!("Stream error from backend: {}", error);
                                let _ = tx
                                    .send(StreamChunk {
                                        chunk_type: "error".to_string(),
                                        content: None,
                                        tool_call: None,
                                        error: Some(error.clone()),
                                        usage: None,
                                        finish_reason: None,
                                        id: None,
                                    })
                                    .await;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse SSE chunk: {} - data: {}", e, data);
                        }
                    }
                }
            }
        }

        Ok(ChatResponse {
            id: response_id,
            content: accumulated_content,
            finish_reason,
            usage: final_usage,
            tool_calls: if accumulated_tool_calls.is_empty() {
                None
            } else {
                Some(accumulated_tool_calls)
            },
        })
    }
}

impl Default for LLMService {
    fn default() -> Self {
        Self::new(None)
    }
}

// Create a singleton service
lazy_static::lazy_static! {
    pub static ref LLM_SERVICE: Arc<LLMService> = Arc::new(LLMService::default());
}
