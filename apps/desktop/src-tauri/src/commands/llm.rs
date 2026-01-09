// LLM Commands - Tauri IPC handlers for LLM functionality

use crate::services::llm_service::{
    AvailableModels, ChatMessage, ChatRequest, ChatResponse, ChatWithToolsRequest, LLMError,
    LLMStatus, QuotaInfo, StreamChunk, ToolDefinition, LLM_SERVICE,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tracing::{debug, error};

// ============================================================================
// Command Input Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatOptions {
    pub provider: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatWithToolsOptions {
    #[serde(flatten)]
    pub base: ChatOptions,
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOptions {
    #[serde(flatten)]
    pub base: ChatOptions,
    pub stream_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamWithToolsOptions {
    #[serde(flatten)]
    pub base: ChatWithToolsOptions,
    pub stream_id: String,
}

// ============================================================================
// Stream Event Types
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEvent {
    pub stream_id: String,
    pub chunk: StreamChunk,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCompleteEvent {
    pub stream_id: String,
    pub response: ChatResponse,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamErrorEvent {
    pub stream_id: String,
    pub error: LLMError,
}

// ============================================================================
// Commands
// ============================================================================

/// Emit session expired event when AUTH_REQUIRED error occurs
fn emit_session_expired_if_auth_error(app: &AppHandle, error: &LLMError) {
    if error.code == "AUTH_REQUIRED" {
        debug!("Emitting auth:session-expired event due to AUTH_REQUIRED error");
        let _ = app.emit("auth:session-expired", ());
    }
}

/// Send a chat message (non-streaming)
#[tauri::command]
pub async fn llm_chat(
    app: AppHandle,
    options: ChatOptions,
    auth_token: Option<String>,
) -> Result<ChatResponse, String> {
    debug!(
        "llm_chat: provider={}, model={}, has_token={}",
        options.provider,
        options.model,
        auth_token.is_some()
    );

    let request = ChatRequest {
        provider: options.provider,
        model: options.model,
        messages: options.messages,
        temperature: options.temperature,
        max_tokens: options.max_tokens,
        stream: Some(false),
        request_type: options.request_type,
        web_search_enabled: options.web_search_enabled,
    };

    LLM_SERVICE
        .chat(request, auth_token.as_deref())
        .await
        .map_err(|e| {
            emit_session_expired_if_auth_error(&app, &e);
            e.to_string()
        })
}

/// Send a streaming chat message
/// Emits 'llm:stream' events with StreamEvent payloads
/// Emits 'llm:stream:complete' on success or 'llm:stream:error' on failure
#[tauri::command]
pub async fn llm_chat_stream(
    app: AppHandle,
    options: StreamOptions,
    auth_token: Option<String>,
) -> Result<(), String> {
    let stream_id = options.stream_id.clone();
    debug!(
        "llm_chat_stream: provider={}, model={}, stream_id={}, has_token={}",
        options.base.provider,
        options.base.model,
        stream_id,
        auth_token.is_some()
    );

    let request = ChatRequest {
        provider: options.base.provider,
        model: options.base.model,
        messages: options.base.messages,
        temperature: options.base.temperature,
        max_tokens: options.base.max_tokens,
        stream: Some(true),
        request_type: options.base.request_type,
        web_search_enabled: options.base.web_search_enabled,
    };

    // Create channel for stream chunks
    let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);

    // Spawn task to forward chunks to frontend
    let app_clone = app.clone();
    let stream_id_clone = stream_id.clone();
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let event = StreamEvent {
                stream_id: stream_id_clone.clone(),
                chunk,
            };
            if let Err(e) = app_clone.emit("llm:stream", &event) {
                error!("Failed to emit stream event: {}", e);
            }
        }
    });

    // Execute the streaming request
    match LLM_SERVICE
        .chat_stream(request, auth_token.as_deref(), tx)
        .await
    {
        Ok(response) => {
            let event = StreamCompleteEvent {
                stream_id: stream_id.clone(),
                response,
            };
            if let Err(e) = app.emit("llm:stream:complete", &event) {
                error!("Failed to emit stream complete event: {}", e);
            }
            Ok(())
        }
        Err(error) => {
            emit_session_expired_if_auth_error(&app, &error);
            let event = StreamErrorEvent {
                stream_id: stream_id.clone(),
                error: error.clone(),
            };
            if let Err(e) = app.emit("llm:stream:error", &event) {
                error!("Failed to emit stream error event: {}", e);
            }
            Err(error.to_string())
        }
    }
}

/// Send a chat message with tools (non-streaming)
#[tauri::command]
pub async fn llm_chat_with_tools(
    app: AppHandle,
    options: ChatWithToolsOptions,
    auth_token: Option<String>,
) -> Result<ChatResponse, String> {
    debug!(
        "llm_chat_with_tools: provider={}, model={}, tools={}",
        options.base.provider,
        options.base.model,
        options.tools.len()
    );

    let request = ChatWithToolsRequest {
        base: ChatRequest {
            provider: options.base.provider,
            model: options.base.model,
            messages: options.base.messages,
            temperature: options.base.temperature,
            max_tokens: options.base.max_tokens,
            stream: Some(false),
            request_type: options.base.request_type,
            web_search_enabled: options.base.web_search_enabled,
        },
        tools: options.tools,
        tool_choice: options.tool_choice,
    };

    LLM_SERVICE
        .chat_with_tools(request, auth_token.as_deref())
        .await
        .map_err(|e| {
            emit_session_expired_if_auth_error(&app, &e);
            e.to_string()
        })
}

/// Send a streaming chat message with tools
#[tauri::command]
pub async fn llm_chat_with_tools_stream(
    app: AppHandle,
    options: StreamWithToolsOptions,
    auth_token: Option<String>,
) -> Result<(), String> {
    let stream_id = options.stream_id.clone();
    debug!(
        "llm_chat_with_tools_stream: provider={}, model={}, tools={}, stream_id={}",
        options.base.base.provider,
        options.base.base.model,
        options.base.tools.len(),
        stream_id
    );

    let request = ChatWithToolsRequest {
        base: ChatRequest {
            provider: options.base.base.provider,
            model: options.base.base.model,
            messages: options.base.base.messages,
            temperature: options.base.base.temperature,
            max_tokens: options.base.base.max_tokens,
            stream: Some(true),
            request_type: options.base.base.request_type,
            web_search_enabled: options.base.base.web_search_enabled,
        },
        tools: options.base.tools,
        tool_choice: options.base.tool_choice,
    };

    // Create channel for stream chunks
    let (tx, mut rx) = mpsc::channel::<StreamChunk>(100);

    // Spawn task to forward chunks to frontend
    let app_clone = app.clone();
    let stream_id_clone = stream_id.clone();
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let event = StreamEvent {
                stream_id: stream_id_clone.clone(),
                chunk,
            };
            if let Err(e) = app_clone.emit("llm:stream", &event) {
                error!("Failed to emit stream event: {}", e);
            }
        }
    });

    // Execute the streaming request
    match LLM_SERVICE
        .chat_with_tools_stream(request, auth_token.as_deref(), tx)
        .await
    {
        Ok(response) => {
            let event = StreamCompleteEvent {
                stream_id: stream_id.clone(),
                response,
            };
            if let Err(e) = app.emit("llm:stream:complete", &event) {
                error!("Failed to emit stream complete event: {}", e);
            }
            Ok(())
        }
        Err(error) => {
            emit_session_expired_if_auth_error(&app, &error);
            let event = StreamErrorEvent {
                stream_id: stream_id.clone(),
                error: error.clone(),
            };
            if let Err(e) = app.emit("llm:stream:error", &event) {
                error!("Failed to emit stream error event: {}", e);
            }
            Err(error.to_string())
        }
    }
}

/// Get available models
#[tauri::command]
pub async fn llm_get_models(auth_token: Option<String>) -> Result<AvailableModels, String> {
    debug!("llm_get_models");

    LLM_SERVICE
        .get_models(auth_token.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Get current quota
#[tauri::command]
pub async fn llm_get_quota(auth_token: Option<String>) -> Result<QuotaInfo, String> {
    debug!("llm_get_quota");

    LLM_SERVICE
        .get_quota(auth_token.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Get LLM service status
#[tauri::command]
pub async fn llm_get_status(auth_token: Option<String>) -> Result<LLMStatus, String> {
    debug!("llm_get_status");

    LLM_SERVICE
        .get_status(auth_token.as_deref())
        .await
        .map_err(|e| e.to_string())
}
