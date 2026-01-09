// Auth Service - Authentication with midlight.ai backend

use cookie_store::CookieStore;
use reqwest::Client;
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const DEFAULT_BASE_URL: &str = "https://midlight.ai";
const TOKEN_REFRESH_BUFFER_SECS: i64 = 60; // Refresh 60 seconds before expiry

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: i64,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub tier: String,
    pub status: String,
    pub billing_interval: Option<String>,
    pub current_period_end: Option<String>,
}

// API response wrapper for subscription endpoint
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionResponse {
    subscription: Subscription,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub used: u32,
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
}

// API response wrapper for usage endpoint
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageResponse {
    quota: Quota,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub id: String,
    pub product_id: String,
    pub name: String,
    pub description: Option<String>,
    pub amount: u32,
    pub currency: String,
    pub interval: String,
    pub features: Option<Vec<String>>,
}

// API response wrapper for prices endpoint
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PricesResponse {
    prices: Vec<Price>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutSession {
    pub url: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalSession {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub user: User,
    pub access_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignupRequest {
    email: String,
    password: String,
    display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeCodeRequest {
    code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum AuthState {
    Initializing,
    Authenticated,
    Unauthenticated,
}

impl std::fmt::Display for AuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthState::Initializing => write!(f, "initializing"),
            AuthState::Authenticated => write!(f, "authenticated"),
            AuthState::Unauthenticated => write!(f, "unauthenticated"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AuthError {}

// ============================================================================
// Auth Service
// ============================================================================

pub struct AuthService {
    client: Client,
    cookie_store: Arc<CookieStoreMutex>,
    app_data_dir: PathBuf,
    base_url: String,
    // In-memory token storage (never persisted to disk)
    access_token: RwLock<Option<String>>,
    token_expiry: RwLock<Option<i64>>, // Unix timestamp
    user: RwLock<Option<User>>,
    auth_state: RwLock<AuthState>,
}

impl AuthService {
    pub fn new(app_data_dir: PathBuf, base_url: Option<String>) -> Self {
        // Load existing cookies from disk
        let cookie_store = Self::load_cookie_store(&app_data_dir);
        let cookie_store = Arc::new(CookieStoreMutex::new(cookie_store));

        // Build default headers for all requests
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            reqwest::header::HeaderName::from_static("x-client-type"),
            reqwest::header::HeaderValue::from_static("desktop"),
        );

        let client = Client::builder()
            .cookie_provider(cookie_store.clone())
            .default_headers(default_headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            cookie_store,
            app_data_dir,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            access_token: RwLock::new(None),
            token_expiry: RwLock::new(None),
            user: RwLock::new(None),
            auth_state: RwLock::new(AuthState::Initializing),
        }
    }

    #[allow(deprecated)]
    fn load_cookie_store(app_data_dir: &Path) -> CookieStore {
        let cookie_path = app_data_dir.join("cookies.json");
        if cookie_path.exists() {
            match std::fs::File::open(&cookie_path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    match CookieStore::load_json(reader) {
                        Ok(store) => {
                            debug!("Loaded cookie store from disk");
                            return store;
                        }
                        Err(e) => {
                            warn!("Failed to parse cookie store: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to open cookie store file: {}", e);
                }
            }
        }
        CookieStore::default()
    }

    #[allow(deprecated)]
    pub fn save_cookies(&self) -> Result<(), AuthError> {
        let cookie_path = self.app_data_dir.join("cookies.json");

        // Ensure directory exists
        if let Some(parent) = cookie_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AuthError {
                code: "STORAGE_ERROR".to_string(),
                message: format!("Failed to create directory: {}", e),
            })?;
        }

        let file = std::fs::File::create(&cookie_path).map_err(|e| AuthError {
            code: "STORAGE_ERROR".to_string(),
            message: format!("Failed to create cookie file: {}", e),
        })?;

        let store = self.cookie_store.lock().unwrap();
        let mut writer = std::io::BufWriter::new(file);
        store.save_json(&mut writer).map_err(|e| AuthError {
            code: "STORAGE_ERROR".to_string(),
            message: format!("Failed to save cookies: {}", e),
        })?;

        debug!("Saved cookie store to disk");
        Ok(())
    }

    fn clear_cookies(&self) -> Result<(), AuthError> {
        let cookie_path = self.app_data_dir.join("cookies.json");
        if cookie_path.exists() {
            std::fs::remove_file(&cookie_path).map_err(|e| AuthError {
                code: "STORAGE_ERROR".to_string(),
                message: format!("Failed to delete cookie file: {}", e),
            })?;
        }

        // Clear in-memory store
        let mut store = self.cookie_store.lock().unwrap();
        store.clear();

        debug!("Cleared cookie store");
        Ok(())
    }

    fn set_tokens(&self, access_token: &str, expires_in: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let expiry = now + (expires_in as i64);

        *self.access_token.write().unwrap() = Some(access_token.to_string());
        *self.token_expiry.write().unwrap() = Some(expiry);
    }

    fn clear_tokens(&self) {
        *self.access_token.write().unwrap() = None;
        *self.token_expiry.write().unwrap() = None;
        *self.user.write().unwrap() = None;
    }

    fn set_auth_state(&self, state: AuthState) {
        *self.auth_state.write().unwrap() = state;
    }

    fn is_token_expired(&self) -> bool {
        let expiry = self.token_expiry.read().unwrap();
        match *expiry {
            Some(exp) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                now >= (exp - TOKEN_REFRESH_BUFFER_SECS)
            }
            None => true,
        }
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Initialize auth service - attempt silent refresh from stored cookie
    pub async fn init(&self) -> Result<AuthState, AuthError> {
        debug!("Initializing auth service");

        // Try to refresh using stored cookie
        match self.refresh_access_token_internal(false).await {
            Ok(response) => {
                self.set_tokens(&response.access_token, response.expires_in);
                *self.user.write().unwrap() = Some(response.user);
                self.set_auth_state(AuthState::Authenticated);
                info!("Auth initialized - user authenticated via refresh");
                Ok(AuthState::Authenticated)
            }
            Err(_) => {
                self.set_auth_state(AuthState::Unauthenticated);
                debug!("Auth initialized - no valid session");
                Ok(AuthState::Unauthenticated)
            }
        }
    }

    /// Email/password signup
    pub async fn signup(
        &self,
        email: &str,
        password: &str,
        display_name: Option<&str>,
    ) -> Result<AuthResponse, AuthError> {
        let url = format!("{}/api/auth/signup", self.base_url);

        let request = SignupRequest {
            email: email.to_string(),
            password: password.to_string(),
            display_name: display_name.map(|s| s.to_string()),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let auth_response: AuthResponse = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: e.to_string(),
        })?;

        // Store tokens and user
        self.set_tokens(&auth_response.access_token, auth_response.expires_in);
        *self.user.write().unwrap() = Some(auth_response.user.clone());
        self.set_auth_state(AuthState::Authenticated);

        // Save cookies (refresh token)
        self.save_cookies()?;

        info!("User signed up: {}", email);
        Ok(auth_response)
    }

    /// Email/password login
    pub async fn login(&self, email: &str, password: &str) -> Result<AuthResponse, AuthError> {
        let url = format!("{}/api/auth/login", self.base_url);

        let request = LoginRequest {
            email: email.to_string(),
            password: password.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let auth_response: AuthResponse = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: e.to_string(),
        })?;

        // Store tokens and user
        self.set_tokens(&auth_response.access_token, auth_response.expires_in);
        *self.user.write().unwrap() = Some(auth_response.user.clone());
        self.set_auth_state(AuthState::Authenticated);

        // Save cookies (refresh token)
        self.save_cookies()?;

        info!("User logged in: {}", email);
        Ok(auth_response)
    }

    /// Logout and clear all tokens
    pub async fn logout(&self) -> Result<(), AuthError> {
        let url = format!("{}/api/auth/logout", self.base_url);

        // Try to notify server (ignore errors)
        let _ = self.client.post(&url).send().await;

        // Clear local state
        self.clear_tokens();
        self.clear_cookies()?;
        self.set_auth_state(AuthState::Unauthenticated);

        info!("User logged out");
        Ok(())
    }

    /// Get current access token, refreshing if needed
    pub async fn get_access_token(&self) -> Option<String> {
        // Check if we have a valid token
        let has_token = self.access_token.read().unwrap().is_some();
        let is_expired = self.is_token_expired();
        debug!(
            "get_access_token: has_token={}, is_expired={}",
            has_token, is_expired
        );

        if !is_expired && has_token {
            return self.access_token.read().unwrap().clone();
        }

        // Try to refresh
        debug!("get_access_token: attempting refresh");
        match self.refresh_access_token().await {
            Ok(_) => {
                debug!("get_access_token: refresh successful");
                self.access_token.read().unwrap().clone()
            }
            Err(e) => {
                warn!("Failed to refresh access token: {}", e);
                None
            }
        }
    }

    /// Refresh access token using stored refresh cookie
    pub async fn refresh_access_token(&self) -> Result<AuthResponse, AuthError> {
        self.refresh_access_token_internal(true).await
    }

    async fn refresh_access_token_internal(
        &self,
        emit_expired: bool,
    ) -> Result<AuthResponse, AuthError> {
        let url = format!("{}/api/auth/refresh", self.base_url);

        let response = self.client.post(&url).send().await.map_err(|e| AuthError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
        })?;

        if !response.status().is_success() {
            let error = self.parse_error_response(response).await;

            if emit_expired {
                // Session expired - clear state
                self.clear_tokens();
                self.set_auth_state(AuthState::Unauthenticated);
            }

            return Err(error);
        }

        let auth_response: AuthResponse = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: e.to_string(),
        })?;

        // Update tokens
        self.set_tokens(&auth_response.access_token, auth_response.expires_in);
        *self.user.write().unwrap() = Some(auth_response.user.clone());

        debug!("Access token refreshed");
        Ok(auth_response)
    }

    /// Exchange OAuth code for tokens
    pub async fn exchange_oauth_code(&self, code: &str) -> Result<AuthResponse, AuthError> {
        let url = format!("{}/api/auth/exchange", self.base_url);

        let request = ExchangeCodeRequest {
            code: code.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let auth_response: AuthResponse = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: e.to_string(),
        })?;

        // Store tokens and user
        self.set_tokens(&auth_response.access_token, auth_response.expires_in);
        *self.user.write().unwrap() = Some(auth_response.user.clone());
        self.set_auth_state(AuthState::Authenticated);

        // Save cookies (refresh token)
        self.save_cookies()?;

        info!("OAuth exchange successful");
        Ok(auth_response)
    }

    /// Build OAuth URL for browser
    pub fn get_oauth_url(&self, callback_port: Option<u16>) -> String {
        let mut url = format!("{}/api/auth/google?desktop=true", self.base_url);

        if let Some(port) = callback_port {
            url.push_str(&format!("&callback_port={}", port));
        }

        url
    }

    /// Get current user
    pub fn get_user(&self) -> Option<User> {
        self.user.read().unwrap().clone()
    }

    /// Get subscription info
    pub async fn get_subscription(&self) -> Result<Subscription, AuthError> {
        let url = format!("{}/api/user/subscription", self.base_url);

        let token = self.get_access_token().await.ok_or_else(|| AuthError {
            code: "NOT_AUTHENTICATED".to_string(),
            message: "No valid access token".to_string(),
        })?;

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let wrapper: SubscriptionResponse = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: format!("error decoding response body: {}", e),
        })?;

        Ok(wrapper.subscription)
    }

    /// Get quota info
    pub async fn get_quota(&self) -> Result<Quota, AuthError> {
        let url = format!("{}/api/user/usage", self.base_url);

        let token = self.get_access_token().await.ok_or_else(|| AuthError {
            code: "NOT_AUTHENTICATED".to_string(),
            message: "No valid access token".to_string(),
        })?;

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let wrapper: UsageResponse = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: format!("error decoding response body: {}", e),
        })?;

        Ok(wrapper.quota)
    }

    /// Get available subscription prices
    pub async fn get_prices(&self) -> Result<Vec<Price>, AuthError> {
        let url = format!("{}/api/subscription/prices", self.base_url);

        let response = self.client.get(&url).send().await.map_err(|e| AuthError {
            code: "NETWORK_ERROR".to_string(),
            message: e.to_string(),
        })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let wrapper: PricesResponse = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: format!("error decoding response body: {}", e),
        })?;

        Ok(wrapper.prices)
    }

    /// Create Stripe checkout session
    pub async fn create_checkout_session(
        &self,
        price_id: &str,
    ) -> Result<CheckoutSession, AuthError> {
        let url = format!("{}/api/subscription/checkout", self.base_url);

        let token = self.get_access_token().await.ok_or_else(|| AuthError {
            code: "NOT_AUTHENTICATED".to_string(),
            message: "No valid access token".to_string(),
        })?;

        let body = serde_json::json!({ "priceId": price_id });

        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: format!("error decoding response body: {}", e),
        })
    }

    /// Create Stripe billing portal session
    pub async fn create_portal_session(&self) -> Result<PortalSession, AuthError> {
        let url = format!("{}/api/subscription/portal", self.base_url);

        let token = self.get_access_token().await.ok_or_else(|| AuthError {
            code: "NOT_AUTHENTICATED".to_string(),
            message: "No valid access token".to_string(),
        })?;

        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: format!("error decoding response body: {}", e),
        })
    }

    /// Request password reset email
    pub async fn forgot_password(&self, email: &str) -> Result<(), AuthError> {
        let url = format!("{}/api/auth/forgot-password", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "email": email }))
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        info!("Password reset email sent to {}", email);
        Ok(())
    }

    /// Reset password with token from email
    pub async fn reset_password(&self, token: &str, new_password: &str) -> Result<(), AuthError> {
        let url = format!("{}/api/auth/reset-password", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "token": token,
                "password": new_password
            }))
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        info!("Password reset successful");
        Ok(())
    }

    /// Update user profile (email, display name, password)
    pub async fn update_profile(
        &self,
        email: Option<&str>,
        display_name: Option<&str>,
        current_password: Option<&str>,
        new_password: Option<&str>,
    ) -> Result<User, AuthError> {
        let url = format!("{}/api/user/profile", self.base_url);

        let token = self.get_access_token().await.ok_or_else(|| AuthError {
            code: "NOT_AUTHENTICATED".to_string(),
            message: "No valid access token".to_string(),
        })?;

        let mut body = serde_json::Map::new();
        if let Some(e) = email {
            body.insert("email".to_string(), serde_json::json!(e));
        }
        if let Some(dn) = display_name {
            body.insert("displayName".to_string(), serde_json::json!(dn));
        }
        if let Some(cp) = current_password {
            body.insert("currentPassword".to_string(), serde_json::json!(cp));
        }
        if let Some(np) = new_password {
            body.insert("newPassword".to_string(), serde_json::json!(np));
        }

        let response = self
            .client
            .patch(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| AuthError {
                code: "NETWORK_ERROR".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let user: User = response.json().await.map_err(|e| AuthError {
            code: "PARSE_ERROR".to_string(),
            message: format!("error decoding response body: {}", e),
        })?;

        // Update stored user
        *self.user.write().unwrap() = Some(user.clone());

        info!("Profile updated successfully");
        Ok(user)
    }

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        *self.auth_state.read().unwrap() == AuthState::Authenticated
    }

    /// Get current auth state
    pub fn get_auth_state(&self) -> AuthState {
        self.auth_state.read().unwrap().clone()
    }

    async fn parse_error_response(&self, response: reqwest::Response) -> AuthError {
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
            403 => "AUTH_EXPIRED",
            404 => "NOT_FOUND",
            409 => "CONFLICT",
            429 => "RATE_LIMITED",
            400 => "INVALID_REQUEST",
            _ if status.is_server_error() => "SERVER_ERROR",
            _ => "UNKNOWN",
        };

        AuthError {
            code: code.to_string(),
            message,
        }
    }
}

// ============================================================================
// Global Singleton
// ============================================================================

lazy_static::lazy_static! {
    pub static ref AUTH_SERVICE: AuthService = {
        // Get app data directory
        let app_data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.midlight.app");

        AuthService::new(app_data_dir, None)
    };
}
