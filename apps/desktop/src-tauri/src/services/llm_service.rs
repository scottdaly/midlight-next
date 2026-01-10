// LLM Service - HTTP client for LLM API communication

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, warn};

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

#[allow(dead_code)]
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

    /// Create a new LLMService with a custom HTTP client (for testing)
    #[cfg(test)]
    pub fn with_client(base_url: String, client: Client) -> Self {
        Self { client, base_url }
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
    async fn handle_response(&self, response: reqwest::Response) -> Result<ChatResponse, LLMError> {
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
        let accumulated_tool_calls: Vec<ToolCall> = Vec::new();
        let mut final_usage: Option<UsageInfo> = None;
        let finish_reason = "stop".to_string();
        let response_id = uuid::Uuid::new_v4().to_string();

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

                if let Some(data) = line.strip_prefix("data: ") {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_service(base_url: &str) -> LLMService {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        LLMService::with_client(base_url.to_string(), client)
    }

    fn create_chat_request() -> ChatRequest {
        ChatRequest {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                name: None,
                tool_call_id: None,
                tool_calls: None,
            }],
            temperature: None,
            max_tokens: None,
            stream: None,
            request_type: None,
            web_search_enabled: None,
        }
    }

    fn mock_chat_response() -> serde_json::Value {
        serde_json::json!({
            "id": "msg_123",
            "content": "Hello! How can I help you?",
            "finishReason": "stop",
            "usage": {
                "promptTokens": 10,
                "completionTokens": 8,
                "totalTokens": 18
            }
        })
    }

    #[tokio::test]
    async fn test_chat_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_chat_response()))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("test_token")).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, "Hello! How can I help you?");
        assert_eq!(response.finish_reason, "stop");
    }

    #[tokio::test]
    async fn test_chat_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(serde_json::json!({
                    "message": "Unauthorized"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, None).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_REQUIRED");
    }

    #[tokio::test]
    async fn test_chat_quota_exceeded() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(serde_json::json!({
                    "code": "QUOTA_EXCEEDED",
                    "message": "Monthly quota exceeded"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "QUOTA_EXCEEDED");
    }

    #[tokio::test]
    async fn test_chat_rate_limited() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(429).set_body_json(serde_json::json!({
                    "message": "Too many requests"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "RATE_LIMITED");
    }

    #[tokio::test]
    async fn test_chat_content_filtered() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(451).set_body_json(serde_json::json!({
                    "message": "Content violates usage policy"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "CONTENT_FILTERED");
    }

    #[tokio::test]
    async fn test_get_models() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": {
                    "openai": [
                        {"id": "gpt-4", "name": "GPT-4", "tier": "pro"}
                    ],
                    "anthropic": [
                        {"id": "claude-3-opus", "name": "Claude 3 Opus", "tier": "pro"}
                    ],
                    "gemini": [
                        {"id": "gemini-pro", "name": "Gemini Pro", "tier": "free"}
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service.get_models(Some("token")).await;

        assert!(result.is_ok());
        let models = result.unwrap();
        assert_eq!(models.openai.len(), 1);
        assert_eq!(models.openai[0].id, "gpt-4");
        assert_eq!(models.anthropic.len(), 1);
        assert_eq!(models.gemini.len(), 1);
    }

    #[tokio::test]
    async fn test_get_quota() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/quota"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tier": "pro",
                "limit": 100000,
                "used": 25000,
                "remaining": 75000,
                "resetsAt": "2024-02-01T00:00:00Z"
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service.get_quota(Some("token")).await;

        assert!(result.is_ok());
        let quota = result.unwrap();
        assert_eq!(quota.tier, "pro");
        assert_eq!(quota.used, 25000);
        assert_eq!(quota.remaining, Some(75000));
    }

    #[tokio::test]
    async fn test_get_status_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "available": true,
                "providers": {
                    "openai": true,
                    "anthropic": true,
                    "gemini": false
                }
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service.get_status(Some("token")).await;

        assert!(result.is_ok());
        let status = result.unwrap();
        assert!(status.available);
        assert!(status.providers.openai);
        assert!(status.providers.anthropic);
        assert!(!status.providers.gemini);
    }

    #[tokio::test]
    async fn test_get_status_unavailable() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/status"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service.get_status(Some("token")).await;

        assert!(result.is_ok());
        let status = result.unwrap();
        assert!(!status.available);
        assert!(!status.providers.openai);
    }

    #[tokio::test]
    async fn test_chat_with_tools() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat-with-tools"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg_456",
                "content": "",
                "finishReason": "tool_calls",
                "toolCalls": [
                    {
                        "id": "call_123",
                        "name": "create_document",
                        "arguments": {"title": "New Doc"}
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let request = ChatWithToolsRequest {
            base: create_chat_request(),
            tools: vec![ToolDefinition {
                name: "create_document".to_string(),
                description: "Create a new document".to_string(),
                parameters: ToolParameters {
                    param_type: "object".to_string(),
                    properties: serde_json::Map::new(),
                    required: None,
                },
            }],
            tool_choice: None,
        };

        let result = service.chat_with_tools(request, Some("token")).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.finish_reason, "tool_calls");
        assert!(response.tool_calls.is_some());
        let tool_calls = response.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "create_document");
    }

    #[tokio::test]
    async fn test_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(500).set_body_json(serde_json::json!({
                    "message": "Internal server error"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PROVIDER_ERROR");
    }

    // ============================================================================
    // Additional Tests
    // ============================================================================

    #[test]
    fn test_chat_message_serialization() {
        let message = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
            name: Some("assistant_name".to_string()),
            tool_call_id: None,
            tool_calls: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
        assert!(json.contains("\"name\":\"assistant_name\""));
        // Optional fields should not be present when None
        assert!(!json.contains("toolCallId"));
    }

    #[test]
    fn test_chat_message_with_tool_calls() {
        let message = ChatMessage {
            role: "assistant".to_string(),
            content: "".to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_123".to_string(),
                name: "create_document".to_string(),
                arguments: serde_json::json!({"title": "Test"}),
            }]),
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"toolCalls\""));
        assert!(json.contains("\"call_123\""));
        assert!(json.contains("\"create_document\""));
    }

    #[test]
    fn test_tool_definition_serialization() {
        let mut properties = serde_json::Map::new();
        properties.insert(
            "title".to_string(),
            serde_json::json!({"type": "string", "description": "Document title"}),
        );

        let tool = ToolDefinition {
            name: "create_document".to_string(),
            description: "Create a new document".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties,
                required: Some(vec!["title".to_string()]),
            },
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"name\":\"create_document\""));
        assert!(json.contains("\"type\":\"object\""));
        assert!(json.contains("\"required\":[\"title\"]"));
    }

    #[test]
    fn test_usage_info_serialization() {
        let usage = UsageInfo {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"promptTokens\":100"));
        assert!(json.contains("\"completionTokens\":50"));
        assert!(json.contains("\"totalTokens\":150"));
    }

    #[test]
    fn test_stream_chunk_content() {
        let chunk = StreamChunk {
            chunk_type: "content".to_string(),
            content: Some("Hello".to_string()),
            tool_call: None,
            error: None,
            usage: None,
            finish_reason: None,
            id: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"type\":\"content\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_stream_chunk_done() {
        let chunk = StreamChunk {
            chunk_type: "done".to_string(),
            content: None,
            tool_call: None,
            error: None,
            usage: Some(UsageInfo {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
            finish_reason: Some("stop".to_string()),
            id: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"type\":\"done\""));
        assert!(json.contains("\"finishReason\":\"stop\""));
        assert!(json.contains("\"usage\""));
    }

    #[test]
    fn test_stream_chunk_error() {
        let chunk = StreamChunk {
            chunk_type: "error".to_string(),
            content: None,
            tool_call: None,
            error: Some("Something went wrong".to_string()),
            usage: None,
            finish_reason: None,
            id: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"error\":\"Something went wrong\""));
    }

    #[test]
    fn test_model_info_serialization() {
        let model = ModelInfo {
            id: "gpt-4".to_string(),
            name: "GPT-4".to_string(),
            tier: "pro".to_string(),
            context_window: Some(128000),
            max_output: Some(4096),
        };

        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("\"id\":\"gpt-4\""));
        assert!(json.contains("\"contextWindow\":128000"));
        assert!(json.contains("\"maxOutput\":4096"));
    }

    #[test]
    fn test_quota_info_serialization() {
        let quota = QuotaInfo {
            tier: "pro".to_string(),
            limit: Some(100000),
            used: 25000,
            remaining: Some(75000),
            resets_at: Some("2024-02-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&quota).unwrap();
        assert!(json.contains("\"tier\":\"pro\""));
        assert!(json.contains("\"limit\":100000"));
        assert!(json.contains("\"used\":25000"));
        assert!(json.contains("\"resetsAt\":\"2024-02-01T00:00:00Z\""));
    }

    #[test]
    fn test_llm_status_serialization() {
        let status = LLMStatus {
            available: true,
            providers: ProviderStatus {
                openai: true,
                anthropic: true,
                gemini: false,
            },
            message: None,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"available\":true"));
        assert!(json.contains("\"openai\":true"));
        assert!(json.contains("\"gemini\":false"));
    }

    #[test]
    fn test_llm_error_display() {
        let error = LLMError {
            code: "TEST_ERROR".to_string(),
            message: "Something failed".to_string(),
            details: None,
        };

        assert_eq!(format!("{}", error), "TEST_ERROR: Something failed");
    }

    #[test]
    fn test_llm_error_with_details() {
        let error = LLMError {
            code: "VALIDATION_ERROR".to_string(),
            message: "Invalid input".to_string(),
            details: Some(serde_json::json!({"field": "messages", "reason": "empty"})),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"details\":{"));
    }

    #[test]
    fn test_chat_request_serialization() {
        let request = ChatRequest {
            provider: "anthropic".to_string(),
            model: "claude-3-opus".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
                name: None,
                tool_call_id: None,
                tool_calls: None,
            }],
            temperature: Some(0.7),
            max_tokens: Some(1000),
            stream: Some(true),
            request_type: Some("chat".to_string()),
            web_search_enabled: Some(true),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"provider\":\"anthropic\""));
        assert!(json.contains("\"model\":\"claude-3-opus\""));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"maxTokens\":1000"));
        assert!(json.contains("\"webSearchEnabled\":true"));
    }

    #[test]
    fn test_chat_response_deserialization() {
        let json = r#"{
            "id": "resp_123",
            "content": "Hello there!",
            "finishReason": "stop",
            "usage": {
                "promptTokens": 10,
                "completionTokens": 5,
                "totalTokens": 15
            }
        }"#;

        let response: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "resp_123");
        assert_eq!(response.content, "Hello there!");
        assert_eq!(response.finish_reason, "stop");
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
    }

    #[test]
    fn test_chat_response_with_tool_calls() {
        let json = r#"{
            "id": "resp_456",
            "content": "",
            "finishReason": "tool_calls",
            "toolCalls": [
                {
                    "id": "call_abc",
                    "name": "search_documents",
                    "arguments": {"query": "test"}
                }
            ]
        }"#;

        let response: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.finish_reason, "tool_calls");
        assert!(response.tool_calls.is_some());
        let tool_calls = response.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "search_documents");
    }

    #[test]
    fn test_llm_service_default() {
        let service = LLMService::default();
        assert_eq!(service.base_url, "https://midlight.ai");
    }

    #[test]
    fn test_llm_service_custom_url() {
        let service = LLMService::new(Some("https://custom.api.com".to_string()));
        assert_eq!(service.base_url, "https://custom.api.com");
    }

    #[tokio::test]
    async fn test_chat_invalid_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "message": "Invalid request parameters"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "INVALID_REQUEST");
    }

    #[tokio::test]
    async fn test_chat_auth_expired() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(serde_json::json!({
                    "message": "Session expired"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_EXPIRED");
    }

    #[tokio::test]
    async fn test_get_models_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/models"))
            .respond_with(
                ResponseTemplate::new(500).set_body_json(serde_json::json!({
                    "message": "Service unavailable"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service.get_models(Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PROVIDER_ERROR");
    }

    #[tokio::test]
    async fn test_get_quota_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/quota"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(serde_json::json!({
                    "message": "Unauthorized"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service.get_quota(None).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_REQUIRED");
    }

    #[test]
    fn test_available_models_deserialization() {
        let json = r#"{
            "openai": [{"id": "gpt-4", "name": "GPT-4", "tier": "pro"}],
            "anthropic": [{"id": "claude-3", "name": "Claude 3", "tier": "pro"}],
            "gemini": []
        }"#;

        let models: AvailableModels = serde_json::from_str(json).unwrap();
        assert_eq!(models.openai.len(), 1);
        assert_eq!(models.anthropic.len(), 1);
        assert_eq!(models.gemini.len(), 0);
    }

    #[test]
    fn test_tool_call_serialization() {
        let tool_call = ToolCall {
            id: "call_abc123".to_string(),
            name: "edit_document".to_string(),
            arguments: serde_json::json!({
                "path": "test.md",
                "content": "New content"
            }),
        };

        let json = serde_json::to_string(&tool_call).unwrap();
        assert!(json.contains("\"id\":\"call_abc123\""));
        assert!(json.contains("\"name\":\"edit_document\""));
        assert!(json.contains("\"arguments\":{"));
    }

    #[test]
    fn test_chat_with_tools_request_serialization() {
        let request = ChatWithToolsRequest {
            base: ChatRequest {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                messages: vec![],
                temperature: None,
                max_tokens: None,
                stream: None,
                request_type: None,
                web_search_enabled: None,
            },
            tools: vec![],
            tool_choice: Some(serde_json::json!("auto")),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"provider\":\"openai\""));
        assert!(json.contains("\"tools\":[]"));
        assert!(json.contains("\"toolChoice\":\"auto\""));
    }

    // ============================================================================
    // Streaming Tests
    // ============================================================================

    #[tokio::test]
    async fn test_chat_stream_success() {
        let mock_server = MockServer::start().await;

        // SSE response with multiple chunks
        let sse_body = "data: {\"content\":\"Hello\"}\n\ndata: {\"content\":\" World\"}\n\ndata: {\"done\":true,\"usage\":{\"promptTokens\":10,\"completionTokens\":5,\"totalTokens\":15}}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let (tx, mut rx) = mpsc::channel::<StreamChunk>(10);
        let result = service.chat_stream(request, Some("token"), tx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, "Hello World");

        // Check received chunks
        let mut chunks = vec![];
        while let Ok(chunk) = rx.try_recv() {
            chunks.push(chunk);
        }

        // Should have content chunks, usage chunk, and done chunk
        assert!(chunks.iter().any(|c| c.chunk_type == "content"));
        assert!(chunks.iter().any(|c| c.chunk_type == "done"));
    }

    #[tokio::test]
    async fn test_chat_stream_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(serde_json::json!({
                    "message": "Unauthorized"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let (tx, _rx) = mpsc::channel::<StreamChunk>(10);
        let result = service.chat_stream(request, None, tx).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_REQUIRED");
    }

    #[tokio::test]
    async fn test_chat_stream_with_error_chunk() {
        let mock_server = MockServer::start().await;

        // SSE response with error chunk
        let sse_body = "data: {\"error\":\"Rate limit exceeded\"}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let (tx, mut rx) = mpsc::channel::<StreamChunk>(10);
        let _result = service.chat_stream(request, Some("token"), tx).await;

        // Check received chunks
        let mut has_error_chunk = false;
        while let Ok(chunk) = rx.try_recv() {
            if chunk.chunk_type == "error" {
                has_error_chunk = true;
                assert_eq!(chunk.error, Some("Rate limit exceeded".to_string()));
            }
        }

        assert!(has_error_chunk);
    }

    #[tokio::test]
    async fn test_chat_with_tools_stream_success() {
        let mock_server = MockServer::start().await;

        let sse_body = "data: {\"content\":\"I will help you\"}\n\ndata: {\"done\":true}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/api/llm/chat-with-tools"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let request = ChatWithToolsRequest {
            base: create_chat_request(),
            tools: vec![],
            tool_choice: None,
        };

        let (tx, mut rx) = mpsc::channel::<StreamChunk>(10);
        let result = service.chat_with_tools_stream(request, Some("token"), tx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, "I will help you");

        // Check received chunks
        let mut chunks = vec![];
        while let Ok(chunk) = rx.try_recv() {
            chunks.push(chunk);
        }

        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    async fn test_chat_with_tools_stream_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat-with-tools"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(serde_json::json!({
                    "code": "QUOTA_EXCEEDED",
                    "message": "Quota exceeded"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let request = ChatWithToolsRequest {
            base: create_chat_request(),
            tools: vec![],
            tool_choice: None,
        };

        let (tx, _rx) = mpsc::channel::<StreamChunk>(10);
        let result = service.chat_with_tools_stream(request, None, tx).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "QUOTA_EXCEEDED");
    }

    #[tokio::test]
    async fn test_chat_stream_empty_response() {
        let mock_server = MockServer::start().await;

        // Empty SSE response - just done
        let sse_body = "data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let (tx, _rx) = mpsc::channel::<StreamChunk>(10);
        let result = service.chat_stream(request, Some("token"), tx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, ""); // No content accumulated
    }

    #[tokio::test]
    async fn test_chat_stream_malformed_json_chunk() {
        let mock_server = MockServer::start().await;

        // SSE with invalid JSON that should be skipped
        let sse_body = "data: {invalid json}\n\ndata: {\"content\":\"Valid\"}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("content-type", "text/event-stream"),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let (tx, _rx) = mpsc::channel::<StreamChunk>(10);
        let result = service.chat_stream(request, Some("token"), tx).await;

        // Should still succeed, skipping malformed chunk
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.content, "Valid");
    }

    // ============================================================================
    // Additional Error Code Tests
    // ============================================================================

    #[tokio::test]
    async fn test_unknown_error_code() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(
                ResponseTemplate::new(418).set_body_json(serde_json::json!({
                    "message": "I'm a teapot"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "UNKNOWN");
    }

    #[tokio::test]
    async fn test_error_without_message() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        // Should fall back to HTTP status
        assert!(error.message.contains("500"));
    }

    #[tokio::test]
    async fn test_error_with_non_json_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PROVIDER_ERROR");
    }

    // ============================================================================
    // Additional Serialization Tests
    // ============================================================================

    #[test]
    fn test_tool_parameter_with_items() {
        let param = ToolParameter {
            param_type: "array".to_string(),
            description: "A list of items".to_string(),
            items: Some(Box::new(ToolParameter {
                param_type: "string".to_string(),
                description: "Item description".to_string(),
                items: None,
                enum_values: None,
            })),
            enum_values: None,
        };

        let json = serde_json::to_string(&param).unwrap();
        assert!(json.contains("\"type\":\"array\""));
        assert!(json.contains("\"items\":{"));
    }

    #[test]
    fn test_tool_parameter_with_enum() {
        let param = ToolParameter {
            param_type: "string".to_string(),
            description: "Status of the item".to_string(),
            items: None,
            enum_values: Some(vec![
                "pending".to_string(),
                "active".to_string(),
                "completed".to_string(),
            ]),
        };

        let json = serde_json::to_string(&param).unwrap();
        assert!(json.contains("\"enum\":[\"pending\",\"active\",\"completed\"]"));
    }

    #[test]
    fn test_provider_status_serialization() {
        let status = ProviderStatus {
            openai: true,
            anthropic: false,
            gemini: true,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"openai\":true"));
        assert!(json.contains("\"anthropic\":false"));
        assert!(json.contains("\"gemini\":true"));
    }

    #[test]
    fn test_llm_status_with_message() {
        let status = LLMStatus {
            available: false,
            providers: ProviderStatus {
                openai: false,
                anthropic: false,
                gemini: false,
            },
            message: Some("Service maintenance in progress".to_string()),
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"message\":\"Service maintenance in progress\""));
    }

    #[test]
    fn test_model_info_minimal() {
        let model = ModelInfo {
            id: "gpt-3.5".to_string(),
            name: "GPT-3.5".to_string(),
            tier: "free".to_string(),
            context_window: None,
            max_output: None,
        };

        let json = serde_json::to_string(&model).unwrap();
        assert!(!json.contains("contextWindow"));
        assert!(!json.contains("maxOutput"));
    }

    #[test]
    fn test_quota_info_unlimited() {
        let quota = QuotaInfo {
            tier: "unlimited".to_string(),
            limit: None,
            used: 500000,
            remaining: None,
            resets_at: None,
        };

        let json = serde_json::to_string(&quota).unwrap();
        assert!(json.contains("\"tier\":\"unlimited\""));
        // None values serialize as null (not skipped) for limit and remaining
        assert!(json.contains("\"limit\":null"));
        assert!(json.contains("\"remaining\":null"));
        // resets_at has skip_serializing_if = "Option::is_none", so it's not included
        assert!(!json.contains("\"resetsAt\""));
    }

    #[test]
    fn test_chat_message_tool_result() {
        let message = ChatMessage {
            role: "tool".to_string(),
            content: "{\"result\": \"success\"}".to_string(),
            name: Some("create_document".to_string()),
            tool_call_id: Some("call_123".to_string()),
            tool_calls: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"role\":\"tool\""));
        assert!(json.contains("\"toolCallId\":\"call_123\""));
        assert!(json.contains("\"name\":\"create_document\""));
    }

    #[test]
    fn test_stream_chunk_with_tool_call() {
        let chunk = StreamChunk {
            chunk_type: "tool_call".to_string(),
            content: None,
            tool_call: Some(ToolCall {
                id: "call_abc".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({"query": "test"}),
            }),
            error: None,
            usage: None,
            finish_reason: None,
            id: Some("chunk_123".to_string()),
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"toolCall\":{"));
        assert!(json.contains("\"id\":\"chunk_123\""));
    }

    #[test]
    fn test_backend_sse_chunk_deserialization() {
        // Test that BackendSSEChunk correctly deserializes
        let json = r#"{"content":"Hello"}"#;
        let chunk: BackendSSEChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.content, Some("Hello".to_string()));
        assert_eq!(chunk.done, None);
        assert!(chunk.usage.is_none());

        let json = r#"{"done":true,"usage":{"promptTokens":10,"completionTokens":5,"totalTokens":15}}"#;
        let chunk: BackendSSEChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.done, Some(true));
        assert!(chunk.usage.is_some());

        let json = r#"{"error":"Something went wrong"}"#;
        let chunk: BackendSSEChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.error, Some("Something went wrong".to_string()));
    }

    // ============================================================================
    // Request Without Auth Token Tests
    // ============================================================================

    #[tokio::test]
    async fn test_chat_without_token() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_chat_response()))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        // Should work without token (for free tier)
        let result = service.chat(request, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_models_without_token() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": {
                    "openai": [],
                    "anthropic": [],
                    "gemini": []
                }
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let result = service.get_models(None).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_quota_without_token() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/quota"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tier": "free",
                "limit": 1000,
                "used": 0,
                "remaining": 1000
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let result = service.get_quota(None).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_status_without_token() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "available": true,
                "providers": {
                    "openai": true,
                    "anthropic": true,
                    "gemini": true
                }
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let result = service.get_status(None).await;

        assert!(result.is_ok());
    }

    // ============================================================================
    // Parse Error Tests
    // ============================================================================

    #[tokio::test]
    async fn test_get_models_parse_error() {
        let mock_server = MockServer::start().await;

        // Return invalid JSON structure
        Mock::given(method("GET"))
            .and(path("/api/llm/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "invalid": "structure"
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let result = service.get_models(Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_get_quota_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/quota"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let result = service.get_quota(Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_get_status_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/llm/status"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid"))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let result = service.get_status(Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_chat_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());
        let request = create_chat_request();

        let result = service.chat(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_chat_with_tools_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/chat-with-tools"))
            .respond_with(
                ResponseTemplate::new(429).set_body_json(serde_json::json!({
                    "message": "Rate limited"
                })),
            )
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let request = ChatWithToolsRequest {
            base: create_chat_request(),
            tools: vec![],
            tool_choice: None,
        };

        let result = service.chat_with_tools(request, Some("token")).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "RATE_LIMITED");
    }

    // ============================================================================
    // Debug and Clone Trait Tests
    // ============================================================================

    #[test]
    fn test_llm_error_is_error() {
        let error = LLMError {
            code: "TEST".to_string(),
            message: "Test error".to_string(),
            details: None,
        };

        // Test that it implements std::error::Error
        let _: &dyn std::error::Error = &error;
    }

    #[test]
    fn test_chat_message_clone() {
        let message = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        };

        let cloned = message.clone();
        assert_eq!(cloned.role, message.role);
        assert_eq!(cloned.content, message.content);
    }

    #[test]
    fn test_chat_request_clone() {
        let request = create_chat_request();
        let cloned = request.clone();
        assert_eq!(cloned.provider, request.provider);
        assert_eq!(cloned.model, request.model);
    }

    #[test]
    fn test_chat_response_clone() {
        let response = ChatResponse {
            id: "test".to_string(),
            content: "content".to_string(),
            finish_reason: "stop".to_string(),
            usage: None,
            tool_calls: None,
        };

        let cloned = response.clone();
        assert_eq!(cloned.id, response.id);
    }

    #[test]
    fn test_stream_chunk_clone() {
        let chunk = StreamChunk {
            chunk_type: "content".to_string(),
            content: Some("test".to_string()),
            tool_call: None,
            error: None,
            usage: None,
            finish_reason: None,
            id: None,
        };

        let cloned = chunk.clone();
        assert_eq!(cloned.chunk_type, chunk.chunk_type);
    }
}
