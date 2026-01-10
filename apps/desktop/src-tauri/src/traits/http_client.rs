//! HTTP client abstraction for testability.
//!
//! Provides a trait for HTTP operations that can be mocked in tests.

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;

/// HTTP response wrapper.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
}

impl HttpResponse {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            body: body.into(),
            headers: HashMap::new(),
        }
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    pub fn text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

/// Error type for HTTP operations.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Result type for HTTP operations.
pub type HttpResult<T> = Result<T, HttpError>;

/// Abstraction over HTTP client operations for testability.
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// Send a GET request.
    async fn get(&self, url: &str) -> HttpResult<HttpResponse>;

    /// Send a GET request with headers.
    async fn get_with_headers(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse>;

    /// Send a POST request with JSON body.
    async fn post_json<T: Serialize + Send + Sync>(
        &self,
        url: &str,
        body: &T,
    ) -> HttpResult<HttpResponse>;

    /// Send a POST request with JSON body and headers.
    async fn post_json_with_headers<T: Serialize + Send + Sync>(
        &self,
        url: &str,
        body: &T,
        headers: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse>;

    /// Send a POST request with form data.
    async fn post_form(
        &self,
        url: &str,
        form: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse>;

    /// Send a PATCH request with JSON body.
    async fn patch_json<T: Serialize + Send + Sync>(
        &self,
        url: &str,
        body: &T,
        headers: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse>;
}

/// Real implementation using reqwest.
#[derive(Debug, Clone)]
pub struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for ReqwestHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn get(&self, url: &str) -> HttpResult<HttpResponse> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(HttpResponse {
            status,
            body,
            headers,
        })
    }

    async fn get_with_headers(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse> {
        let mut request = self.client.get(url);
        for (key, value) in headers {
            request = request.header(key, value);
        }

        let response = request
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();
        let resp_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(HttpResponse {
            status,
            body,
            headers: resp_headers,
        })
    }

    async fn post_json<T: Serialize + Send + Sync>(
        &self,
        url: &str,
        body: &T,
    ) -> HttpResult<HttpResponse> {
        let response = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(HttpResponse {
            status,
            body,
            headers,
        })
    }

    async fn post_json_with_headers<T: Serialize + Send + Sync>(
        &self,
        url: &str,
        body: &T,
        headers: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse> {
        let mut request = self.client.post(url).json(body);
        for (key, value) in headers {
            request = request.header(key, value);
        }

        let response = request
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();
        let resp_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(HttpResponse {
            status,
            body,
            headers: resp_headers,
        })
    }

    async fn post_form(
        &self,
        url: &str,
        form: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse> {
        let response = self
            .client
            .post(url)
            .form(form)
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(HttpResponse {
            status,
            body,
            headers,
        })
    }

    async fn patch_json<T: Serialize + Send + Sync>(
        &self,
        url: &str,
        body: &T,
        headers: &HashMap<String, String>,
    ) -> HttpResult<HttpResponse> {
        let mut request = self.client.patch(url).json(body);
        for (key, value) in headers {
            request = request.header(key, value);
        }

        let response = request
            .send()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();
        let resp_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| HttpError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(HttpResponse {
            status,
            body,
            headers: resp_headers,
        })
    }
}

/// Mock implementation for testing.
#[cfg(test)]
pub use mock::MockHttpClient;

#[cfg(test)]
mod mock {
    use super::*;
    use std::sync::{Arc, RwLock};

    /// Mock HTTP client for testing.
    #[derive(Debug, Clone, Default)]
    pub struct MockHttpClient {
        responses: Arc<RwLock<Vec<HttpResponse>>>,
        requests: Arc<RwLock<Vec<MockRequest>>>,
    }

    #[derive(Debug, Clone)]
    pub struct MockRequest {
        pub method: String,
        pub url: String,
        pub body: Option<String>,
        pub headers: HashMap<String, String>,
    }

    impl MockHttpClient {
        pub fn new() -> Self {
            Self {
                responses: Arc::new(RwLock::new(Vec::new())),
                requests: Arc::new(RwLock::new(Vec::new())),
            }
        }

        /// Queue a response to be returned by the next request.
        pub fn queue_response(self, response: HttpResponse) -> Self {
            self.responses.write().unwrap().push(response);
            self
        }

        /// Queue a JSON response.
        pub fn queue_json_response<T: Serialize>(self, status: u16, body: &T) -> Self {
            let json = serde_json::to_vec(body).unwrap();
            self.queue_response(
                HttpResponse::new(status, json).with_header("content-type", "application/json"),
            )
        }

        /// Queue an error response.
        pub fn queue_error_response(self, status: u16, message: &str) -> Self {
            self.queue_response(HttpResponse::new(status, message.as_bytes().to_vec()))
        }

        /// Get all recorded requests.
        pub fn get_requests(&self) -> Vec<MockRequest> {
            self.requests.read().unwrap().clone()
        }

        /// Get the last request made.
        pub fn last_request(&self) -> Option<MockRequest> {
            self.requests.read().unwrap().last().cloned()
        }

        fn record_request(&self, method: &str, url: &str, body: Option<String>, headers: HashMap<String, String>) {
            self.requests.write().unwrap().push(MockRequest {
                method: method.to_string(),
                url: url.to_string(),
                body,
                headers,
            });
        }

        fn next_response(&self) -> HttpResult<HttpResponse> {
            let mut responses = self.responses.write().unwrap();
            if responses.is_empty() {
                // Return a default 200 response if none queued
                Ok(HttpResponse::new(200, Vec::new()))
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn get(&self, url: &str) -> HttpResult<HttpResponse> {
            self.record_request("GET", url, None, HashMap::new());
            self.next_response()
        }

        async fn get_with_headers(
            &self,
            url: &str,
            headers: &HashMap<String, String>,
        ) -> HttpResult<HttpResponse> {
            self.record_request("GET", url, None, headers.clone());
            self.next_response()
        }

        async fn post_json<T: Serialize + Send + Sync>(
            &self,
            url: &str,
            body: &T,
        ) -> HttpResult<HttpResponse> {
            let body_str = serde_json::to_string(body)
                .map_err(|e| HttpError::SerializationError(e.to_string()))?;
            self.record_request("POST", url, Some(body_str), HashMap::new());
            self.next_response()
        }

        async fn post_json_with_headers<T: Serialize + Send + Sync>(
            &self,
            url: &str,
            body: &T,
            headers: &HashMap<String, String>,
        ) -> HttpResult<HttpResponse> {
            let body_str = serde_json::to_string(body)
                .map_err(|e| HttpError::SerializationError(e.to_string()))?;
            self.record_request("POST", url, Some(body_str), headers.clone());
            self.next_response()
        }

        async fn post_form(
            &self,
            url: &str,
            form: &HashMap<String, String>,
        ) -> HttpResult<HttpResponse> {
            let body_str = serde_json::to_string(form)
                .map_err(|e| HttpError::SerializationError(e.to_string()))?;
            self.record_request("POST_FORM", url, Some(body_str), HashMap::new());
            self.next_response()
        }

        async fn patch_json<T: Serialize + Send + Sync>(
            &self,
            url: &str,
            body: &T,
            headers: &HashMap<String, String>,
        ) -> HttpResult<HttpResponse> {
            let body_str = serde_json::to_string(body)
                .map_err(|e| HttpError::SerializationError(e.to_string()))?;
            self.record_request("PATCH", url, Some(body_str), headers.clone());
            self.next_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_mock_http_client_get() {
        let client = MockHttpClient::new()
            .queue_response(HttpResponse::new(200, b"Hello, World!".to_vec()));

        let response = client.get("https://example.com").await.unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(response.text().unwrap(), "Hello, World!");

        let requests = client.get_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].url, "https://example.com");
    }

    #[tokio::test]
    async fn test_mock_http_client_post_json() {
        let client = MockHttpClient::new().queue_json_response(201, &json!({"id": 123}));

        let body = json!({"name": "test"});
        let response = client
            .post_json("https://api.example.com/users", &body)
            .await
            .unwrap();

        assert_eq!(response.status, 201);
        let resp_body: serde_json::Value = response.json().unwrap();
        assert_eq!(resp_body["id"], 123);

        let request = client.last_request().unwrap();
        assert_eq!(request.method, "POST");
        assert!(request.body.unwrap().contains("\"name\":\"test\""));
    }

    #[tokio::test]
    async fn test_mock_http_client_multiple_responses() {
        let client = MockHttpClient::new()
            .queue_response(HttpResponse::new(200, b"First".to_vec()))
            .queue_response(HttpResponse::new(201, b"Second".to_vec()));

        let resp1 = client.get("https://example.com/1").await.unwrap();
        let resp2 = client.get("https://example.com/2").await.unwrap();

        assert_eq!(resp1.status, 200);
        assert_eq!(resp1.text().unwrap(), "First");
        assert_eq!(resp2.status, 201);
        assert_eq!(resp2.text().unwrap(), "Second");
    }
}
