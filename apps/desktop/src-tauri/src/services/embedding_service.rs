// Embedding Service - HTTP client for embedding API
//
// Calls the midlight.ai embedding endpoint to generate vector embeddings
// for document chunks.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info};

const DEFAULT_BASE_URL: &str = "https://midlight.ai";

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize)]
struct EmbedRequest {
    texts: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
    model: String,
    dimensions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for EmbeddingError {}

// ============================================================================
// Embedding Service
// ============================================================================

pub struct EmbeddingService {
    client: Client,
    base_url: String,
}

impl EmbeddingService {
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

    /// Generate embeddings for a batch of texts
    ///
    /// # Arguments
    /// * `texts` - Array of texts to embed (max 100 per request)
    /// * `auth_token` - User's authentication token
    ///
    /// # Returns
    /// Vector of embedding vectors, one per input text
    pub async fn embed_texts(
        &self,
        texts: Vec<String>,
        auth_token: &str,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // API limit is 100 texts per request, so batch if needed
        const MAX_BATCH_SIZE: usize = 100;
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for batch in texts.chunks(MAX_BATCH_SIZE) {
            let batch_embeddings = self.embed_batch(batch.to_vec(), auth_token).await?;
            all_embeddings.extend(batch_embeddings);
        }

        Ok(all_embeddings)
    }

    /// Embed a single batch (internal)
    async fn embed_batch(
        &self,
        texts: Vec<String>,
        auth_token: &str,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let url = format!("{}/api/llm/embed", self.base_url);

        debug!("Embedding {} texts", texts.len());

        let response = self
            .client
            .post(&url)
            .bearer_auth(auth_token)
            .json(&EmbedRequest { texts: texts.clone() })
            .send()
            .await
            .map_err(|e| EmbeddingError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body: Option<serde_json::Value> = response.json().await.ok();

            let message = error_body
                .as_ref()
                .and_then(|b| b.get("error"))
                .and_then(|m| m.as_str())
                .unwrap_or(&format!("HTTP {}", status))
                .to_string();

            let code = match status.as_u16() {
                401 => "AUTH_REQUIRED",
                403 => "AUTH_EXPIRED",
                429 => {
                    if message.contains("quota") {
                        "QUOTA_EXCEEDED"
                    } else {
                        "RATE_LIMITED"
                    }
                }
                400 => "INVALID_REQUEST",
                _ if status.is_server_error() => "SERVER_ERROR",
                _ => "UNKNOWN",
            };

            error!("Embedding API error {}: {}", code, message);

            return Err(EmbeddingError {
                code: code.to_string(),
                message,
            });
        }

        let result: EmbedResponse = response.json().await.map_err(|e| EmbeddingError {
            code: "PARSE_ERROR".to_string(),
            message: format!("Failed to parse response: {}", e),
        })?;

        info!(
            "Generated {} embeddings (model: {}, dimensions: {})",
            result.embeddings.len(),
            result.model,
            result.dimensions
        );

        Ok(result.embeddings)
    }

    /// Embed a single query string
    ///
    /// # Arguments
    /// * `query` - The search query to embed
    /// * `auth_token` - User's authentication token
    ///
    /// # Returns
    /// Single embedding vector
    pub async fn embed_query(
        &self,
        query: &str,
        auth_token: &str,
    ) -> Result<Vec<f32>, EmbeddingError> {
        let embeddings = self.embed_texts(vec![query.to_string()], auth_token).await?;

        embeddings.into_iter().next().ok_or_else(|| EmbeddingError {
            code: "NO_EMBEDDING".to_string(),
            message: "No embedding returned for query".to_string(),
        })
    }
}

impl Default for EmbeddingService {
    fn default() -> Self {
        Self::new(None)
    }
}

// Create a singleton service
lazy_static::lazy_static! {
    pub static ref EMBEDDING_SERVICE: Arc<EmbeddingService> = Arc::new(EmbeddingService::default());
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_service(base_url: &str) -> EmbeddingService {
        EmbeddingService::new(Some(base_url.to_string()))
    }

    #[tokio::test]
    async fn test_embed_texts_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]],
                "model": "text-embedding-3-small",
                "dimensions": 1536
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service
            .embed_texts(
                vec!["Hello".to_string(), "World".to_string()],
                "test_token",
            )
            .await;

        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0], vec![0.1, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn test_embed_empty_texts() {
        let service = EmbeddingService::default();

        let result = service.embed_texts(vec![], "test_token").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_embed_query() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2, 0.3]],
                "model": "text-embedding-3-small",
                "dimensions": 1536
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service.embed_query("test query", "test_token").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0.1, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn test_embed_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/embed"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "Unauthorized"
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service
            .embed_texts(vec!["Hello".to_string()], "invalid_token")
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_REQUIRED");
    }

    #[tokio::test]
    async fn test_embed_quota_exceeded() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/embed"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": "Monthly quota exceeded"
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service
            .embed_texts(vec!["Hello".to_string()], "test_token")
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "QUOTA_EXCEEDED");
    }

    #[tokio::test]
    async fn test_embed_rate_limited() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/llm/embed"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": "Too many requests"
            })))
            .mount(&mock_server)
            .await;

        let service = create_test_service(&mock_server.uri());

        let result = service
            .embed_texts(vec!["Hello".to_string()], "test_token")
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "RATE_LIMITED");
    }

    #[test]
    fn test_embedding_error_display() {
        let error = EmbeddingError {
            code: "TEST_ERROR".to_string(),
            message: "Something failed".to_string(),
        };

        assert_eq!(format!("{}", error), "TEST_ERROR: Something failed");
    }
}
