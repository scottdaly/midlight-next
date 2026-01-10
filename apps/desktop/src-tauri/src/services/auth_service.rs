// Auth Service - Authentication with midlight.ai backend

use cookie_store::CookieStore;
use reqwest::Client;
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

use crate::traits::{RealTimeProvider, TimeProvider};

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

pub struct AuthService<T: TimeProvider = RealTimeProvider> {
    client: Client,
    cookie_store: Arc<CookieStoreMutex>,
    app_data_dir: PathBuf,
    base_url: String,
    time_provider: Arc<T>,
    // In-memory token storage (never persisted to disk)
    access_token: RwLock<Option<String>>,
    token_expiry: RwLock<Option<i64>>, // Unix timestamp
    user: RwLock<Option<User>>,
    auth_state: RwLock<AuthState>,
}

/// Type alias for production use
#[allow(dead_code)]
pub type ProductionAuthService = AuthService<RealTimeProvider>;

impl AuthService<RealTimeProvider> {
    pub fn new(app_data_dir: PathBuf, base_url: Option<String>) -> Self {
        Self::with_time_provider(app_data_dir, base_url, Arc::new(RealTimeProvider))
    }
}

impl<T: TimeProvider> AuthService<T> {
    /// Create a new AuthService with custom time provider (for testing)
    pub fn with_time_provider(
        app_data_dir: PathBuf,
        base_url: Option<String>,
        time_provider: Arc<T>,
    ) -> Self {
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
            time_provider,
            access_token: RwLock::new(None),
            token_expiry: RwLock::new(None),
            user: RwLock::new(None),
            auth_state: RwLock::new(AuthState::Initializing),
        }
    }

    /// Create a new AuthService for testing with a custom HTTP client
    #[cfg(test)]
    pub fn with_client_for_testing(
        app_data_dir: PathBuf,
        base_url: String,
        client: Client,
        time_provider: Arc<T>,
    ) -> Self {
        let cookie_store = Arc::new(CookieStoreMutex::new(CookieStore::default()));

        Self {
            client,
            cookie_store,
            app_data_dir,
            base_url,
            time_provider,
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
        let now = self.time_provider.unix_timestamp();
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
                let now = self.time_provider.unix_timestamp();
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
    pub static ref AUTH_SERVICE: AuthService<RealTimeProvider> = {
        // Get app data directory
        let app_data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.midlight.app");

        AuthService::new(app_data_dir, None)
    };
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::time::MockTimeProvider;
    use tempfile::tempdir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_service(
        base_url: &str,
        time_provider: Arc<MockTimeProvider>,
    ) -> AuthService<MockTimeProvider> {
        let temp = tempdir().unwrap();
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        AuthService::with_client_for_testing(
            temp.path().to_path_buf(),
            base_url.to_string(),
            client,
            time_provider,
        )
    }

    fn mock_auth_response() -> serde_json::Value {
        serde_json::json!({
            "user": {
                "id": 1,
                "email": "test@example.com",
                "displayName": "Test User",
                "avatarUrl": null
            },
            "accessToken": "mock_access_token",
            "expiresIn": 3600
        })
    }

    #[tokio::test]
    async fn test_login_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.user.email, "test@example.com");
        assert_eq!(response.access_token, "mock_access_token");
        assert!(service.is_authenticated());
    }

    #[tokio::test]
    async fn test_login_invalid_credentials() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Invalid credentials"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "wrong_password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_REQUIRED");
    }

    #[tokio::test]
    async fn test_signup_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/signup"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service
            .signup("test@example.com", "password", Some("Test User"))
            .await;

        assert!(result.is_ok());
        assert!(service.is_authenticated());
    }

    #[tokio::test]
    async fn test_token_expiry_detection() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider.clone(),
        );

        // Set tokens with 3600 second expiry (at time 1704067200)
        service.set_tokens("test_token", 3600);

        // Token should be valid initially
        assert!(!service.is_token_expired());

        // Advance time by 3539 seconds (still within buffer - 1 second before buffer threshold)
        time_provider.advance_secs(3539);
        assert!(!service.is_token_expired());

        // Advance 1 more second - now at buffer (60 secs before expiry)
        time_provider.advance_secs(1);
        assert!(service.is_token_expired());
    }

    #[tokio::test]
    async fn test_logout_clears_state() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/auth/logout"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        // Login first
        service.login("test@example.com", "password").await.unwrap();
        assert!(service.is_authenticated());

        // Logout
        service.logout().await.unwrap();
        assert!(!service.is_authenticated());
        assert!(service.get_user().is_none());
    }

    #[tokio::test]
    async fn test_get_oauth_url() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://midlight.ai".to_string()),
            time_provider,
        );

        let url = service.get_oauth_url(None);
        assert_eq!(url, "https://midlight.ai/api/auth/google?desktop=true");

        let url_with_port = service.get_oauth_url(Some(8080));
        assert_eq!(
            url_with_port,
            "https://midlight.ai/api/auth/google?desktop=true&callback_port=8080"
        );
    }

    #[tokio::test]
    async fn test_auth_state_transitions() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        // Initial state
        assert_eq!(service.get_auth_state(), AuthState::Initializing);

        // After setting unauthenticated
        service.set_auth_state(AuthState::Unauthenticated);
        assert_eq!(service.get_auth_state(), AuthState::Unauthenticated);

        // After setting authenticated
        service.set_auth_state(AuthState::Authenticated);
        assert_eq!(service.get_auth_state(), AuthState::Authenticated);
    }

    #[tokio::test]
    async fn test_get_subscription() {
        let mock_server = MockServer::start().await;

        // Login first
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/user/subscription"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "subscription": {
                    "tier": "pro",
                    "status": "active",
                    "billingInterval": "monthly",
                    "currentPeriodEnd": "2024-02-01T00:00:00Z"
                }
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let subscription = service.get_subscription().await.unwrap();

        assert_eq!(subscription.tier, "pro");
        assert_eq!(subscription.status, "active");
    }

    #[tokio::test]
    async fn test_get_quota() {
        let mock_server = MockServer::start().await;

        // Login first
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/user/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "quota": {
                    "used": 100,
                    "limit": 1000,
                    "remaining": 900
                }
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let quota = service.get_quota().await.unwrap();

        assert_eq!(quota.used, 100);
        assert_eq!(quota.limit, Some(1000));
        assert_eq!(quota.remaining, Some(900));
    }

    #[tokio::test]
    async fn test_rate_limited_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "message": "Too many requests"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "RATE_LIMITED");
    }

    // ============================================================================
    // Additional Tests
    // ============================================================================

    #[test]
    fn test_user_serialization() {
        let user = User {
            id: 123,
            email: "test@example.com".to_string(),
            display_name: Some("Test User".to_string()),
            avatar_url: None,
        };

        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains("\"id\":123"));
        assert!(json.contains("\"email\":\"test@example.com\""));
        assert!(json.contains("\"displayName\":\"Test User\""));
    }

    #[test]
    fn test_user_deserialization() {
        let json = r#"{"id":456,"email":"user@test.com","displayName":null,"avatarUrl":"https://example.com/avatar.png"}"#;
        let user: User = serde_json::from_str(json).unwrap();

        assert_eq!(user.id, 456);
        assert_eq!(user.email, "user@test.com");
        assert!(user.display_name.is_none());
        assert_eq!(
            user.avatar_url,
            Some("https://example.com/avatar.png".to_string())
        );
    }

    #[test]
    fn test_subscription_serialization() {
        let sub = Subscription {
            tier: "pro".to_string(),
            status: "active".to_string(),
            billing_interval: Some("monthly".to_string()),
            current_period_end: Some("2024-02-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&sub).unwrap();
        assert!(json.contains("\"tier\":\"pro\""));
        assert!(json.contains("\"billingInterval\":\"monthly\""));
    }

    #[test]
    fn test_quota_serialization() {
        let quota = Quota {
            used: 100,
            limit: Some(1000),
            remaining: Some(900),
        };

        let json = serde_json::to_string(&quota).unwrap();
        assert!(json.contains("\"used\":100"));
        assert!(json.contains("\"limit\":1000"));
    }

    #[test]
    fn test_auth_state_display() {
        assert_eq!(format!("{}", AuthState::Initializing), "initializing");
        assert_eq!(format!("{}", AuthState::Authenticated), "authenticated");
        assert_eq!(format!("{}", AuthState::Unauthenticated), "unauthenticated");
    }

    #[test]
    fn test_auth_error_display() {
        let error = AuthError {
            code: "TEST_ERROR".to_string(),
            message: "Something went wrong".to_string(),
        };

        assert_eq!(format!("{}", error), "TEST_ERROR: Something went wrong");
    }

    #[test]
    fn test_price_serialization() {
        let price = Price {
            id: "price_123".to_string(),
            product_id: "prod_abc".to_string(),
            name: "Pro Monthly".to_string(),
            description: Some("Pro plan billed monthly".to_string()),
            amount: 999,
            currency: "usd".to_string(),
            interval: "month".to_string(),
            features: Some(vec!["Feature 1".to_string(), "Feature 2".to_string()]),
        };

        let json = serde_json::to_string(&price).unwrap();
        assert!(json.contains("\"id\":\"price_123\""));
        assert!(json.contains("\"amount\":999"));
    }

    #[tokio::test]
    async fn test_refresh_token_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.refresh_access_token().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.access_token, "mock_access_token");
    }

    #[tokio::test]
    async fn test_refresh_token_expired() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Session expired"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        // Set tokens first
        service.set_tokens("old_token", 3600);
        service.set_auth_state(AuthState::Authenticated);

        let result = service.refresh_access_token().await;

        assert!(result.is_err());
        // State should be cleared after failed refresh
        assert_eq!(service.get_auth_state(), AuthState::Unauthenticated);
    }

    #[tokio::test]
    async fn test_exchange_oauth_code() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/exchange"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.exchange_oauth_code("oauth_code_123").await;

        assert!(result.is_ok());
        assert!(service.is_authenticated());
    }

    #[tokio::test]
    async fn test_forgot_password() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/forgot-password"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.forgot_password("test@example.com").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reset_password() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/reset-password"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.reset_password("reset_token", "new_password").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_prices() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/subscription/prices"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "prices": [
                    {
                        "id": "price_monthly",
                        "productId": "prod_pro",
                        "name": "Pro Monthly",
                        "description": "Monthly subscription",
                        "amount": 999,
                        "currency": "usd",
                        "interval": "month",
                        "features": ["Unlimited AI", "Priority support"]
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.get_prices().await;

        assert!(result.is_ok());
        let prices = result.unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].id, "price_monthly");
        assert_eq!(prices[0].amount, 999);
    }

    #[tokio::test]
    async fn test_create_checkout_session() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/subscription/checkout"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "url": "https://checkout.stripe.com/session_123",
                "sessionId": "cs_test_123"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.create_checkout_session("price_123").await;

        assert!(result.is_ok());
        let session = result.unwrap();
        assert!(session.url.contains("checkout.stripe.com"));
    }

    #[tokio::test]
    async fn test_create_portal_session() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/subscription/portal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "url": "https://billing.stripe.com/portal_123"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.create_portal_session().await;

        assert!(result.is_ok());
        let portal = result.unwrap();
        assert!(portal.url.contains("billing.stripe.com"));
    }

    #[tokio::test]
    async fn test_update_profile() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("PATCH"))
            .and(path("/api/user/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 1,
                "email": "new@example.com",
                "displayName": "New Name",
                "avatarUrl": null
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service
            .update_profile(Some("new@example.com"), Some("New Name"), None, None)
            .await;

        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.email, "new@example.com");
        assert_eq!(user.display_name, Some("New Name".to_string()));
    }

    #[tokio::test]
    async fn test_server_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "message": "Internal server error"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "SERVER_ERROR");
    }

    #[tokio::test]
    async fn test_conflict_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/signup"))
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "message": "Email already exists"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service
            .signup("existing@example.com", "password", None)
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "CONFLICT");
    }

    #[tokio::test]
    async fn test_get_user_before_login() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        assert!(service.get_user().is_none());
    }

    #[tokio::test]
    async fn test_get_user_after_login() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();

        let user = service.get_user();
        assert!(user.is_some());
        assert_eq!(user.unwrap().email, "test@example.com");
    }

    #[tokio::test]
    async fn test_init_without_stored_session() {
        let mock_server = MockServer::start().await;

        // Refresh should fail (no stored cookies)
        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.init().await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), AuthState::Unauthenticated);
    }

    #[tokio::test]
    async fn test_get_subscription_not_authenticated() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service.get_subscription().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_AUTHENTICATED");
    }

    #[tokio::test]
    async fn test_clear_tokens() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        // Set tokens
        service.set_tokens("test_token", 3600);
        *service.user.write().unwrap() = Some(User {
            id: 1,
            email: "test@example.com".to_string(),
            display_name: None,
            avatar_url: None,
        });

        // Clear tokens
        service.clear_tokens();

        assert!(service.access_token.read().unwrap().is_none());
        assert!(service.token_expiry.read().unwrap().is_none());
        assert!(service.user.read().unwrap().is_none());
    }

    #[test]
    fn test_auth_response_serialization() {
        let response = AuthResponse {
            user: User {
                id: 1,
                email: "test@example.com".to_string(),
                display_name: None,
                avatar_url: None,
            },
            access_token: "token123".to_string(),
            expires_in: 3600,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"accessToken\":\"token123\""));
        assert!(json.contains("\"expiresIn\":3600"));
    }

    #[test]
    fn test_checkout_session_serialization() {
        let session = CheckoutSession {
            url: "https://checkout.stripe.com/test".to_string(),
            session_id: Some("cs_123".to_string()),
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"url\":\"https://checkout.stripe.com/test\""));
        assert!(json.contains("\"sessionId\":\"cs_123\""));
    }

    #[test]
    fn test_portal_session_serialization() {
        let session = PortalSession {
            url: "https://billing.stripe.com/portal".to_string(),
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"url\":\"https://billing.stripe.com/portal\""));
    }

    // ============================================================================
    // Cookie Store Tests
    // ============================================================================

    #[test]
    fn test_save_cookies_creates_file() {
        let temp = tempdir().unwrap();
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service.save_cookies();
        assert!(result.is_ok());

        // Verify file was created
        let cookie_path = temp.path().join("cookies.json");
        assert!(cookie_path.exists());
    }

    #[test]
    fn test_load_cookie_store_nonexistent_file() {
        let temp = tempdir().unwrap();
        let store = AuthService::<RealTimeProvider>::load_cookie_store(temp.path());

        // Should return default empty store
        assert!(store.iter_any().count() == 0);
    }

    #[test]
    fn test_load_cookie_store_invalid_json() {
        let temp = tempdir().unwrap();
        let cookie_path = temp.path().join("cookies.json");

        // Write invalid JSON
        std::fs::write(&cookie_path, "not valid json").unwrap();

        let store = AuthService::<RealTimeProvider>::load_cookie_store(temp.path());

        // Should return default empty store on parse error
        assert!(store.iter_any().count() == 0);
    }

    #[test]
    fn test_clear_cookies_removes_file() {
        let temp = tempdir().unwrap();
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        // Save cookies first
        service.save_cookies().unwrap();
        let cookie_path = temp.path().join("cookies.json");
        assert!(cookie_path.exists());

        // Clear cookies
        let result = service.clear_cookies();
        assert!(result.is_ok());
        assert!(!cookie_path.exists());
    }

    #[test]
    fn test_clear_cookies_nonexistent_file_ok() {
        let temp = tempdir().unwrap();
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        // Clear without saving first - should succeed
        let result = service.clear_cookies();
        assert!(result.is_ok());
    }

    // ============================================================================
    // Token Expiry Edge Cases
    // ============================================================================

    #[test]
    fn test_is_token_expired_no_expiry_set() {
        let temp = tempdir().unwrap();
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        // Without setting tokens, should be expired
        assert!(service.is_token_expired());
    }

    #[tokio::test]
    async fn test_get_access_token_valid_token() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();

        // Should return token without refreshing
        let token = service.get_access_token().await;
        assert!(token.is_some());
        assert_eq!(token.unwrap(), "mock_access_token");
    }

    #[tokio::test]
    async fn test_get_access_token_expired_refreshes() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user": {
                    "id": 1,
                    "email": "test@example.com",
                    "displayName": null,
                    "avatarUrl": null
                },
                "accessToken": "refreshed_token",
                "expiresIn": 3600
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider.clone());

        // Set expired token
        service.set_tokens("old_token", 100);
        time_provider.advance_secs(200); // Past expiry

        let token = service.get_access_token().await;
        assert!(token.is_some());
        assert_eq!(token.unwrap(), "refreshed_token");
    }

    #[tokio::test]
    async fn test_get_access_token_refresh_fails() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider.clone());

        // Set expired token
        service.set_tokens("old_token", 100);
        time_provider.advance_secs(200);

        let token = service.get_access_token().await;
        assert!(token.is_none());
    }

    // ============================================================================
    // Init Tests
    // ============================================================================

    #[tokio::test]
    async fn test_init_with_valid_session() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.init().await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), AuthState::Authenticated);
        assert!(service.is_authenticated());
    }

    // ============================================================================
    // Error Response Parsing Tests
    // ============================================================================

    #[tokio::test]
    async fn test_error_response_403_auth_expired() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "Token expired"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_EXPIRED");
    }

    #[tokio::test]
    async fn test_error_response_404_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "User not found"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_FOUND");
    }

    #[tokio::test]
    async fn test_error_response_400_invalid_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "message": "Invalid email format"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("invalid-email", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "INVALID_REQUEST");
    }

    #[tokio::test]
    async fn test_error_response_unknown_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(418).set_body_json(serde_json::json!({
                "message": "I'm a teapot"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "UNKNOWN");
    }

    #[tokio::test]
    async fn test_error_response_no_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_REQUIRED");
        assert!(error.message.contains("HTTP 401"));
    }

    #[tokio::test]
    async fn test_error_response_no_message_field() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "Something went wrong"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "SERVER_ERROR");
        assert!(error.message.contains("HTTP 500"));
    }

    // ============================================================================
    // Signup Edge Cases
    // ============================================================================

    #[tokio::test]
    async fn test_signup_without_display_name() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/signup"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.signup("test@example.com", "password", None).await;

        assert!(result.is_ok());
        assert!(service.is_authenticated());
    }

    // ============================================================================
    // Refresh Token Internal Tests
    // ============================================================================

    #[tokio::test]
    async fn test_refresh_internal_without_emit_expired() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Session expired"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        // Set to authenticated first
        service.set_auth_state(AuthState::Authenticated);
        service.set_tokens("old_token", 3600);

        // Call internal method with emit_expired = false
        let result = service.refresh_access_token_internal(false).await;

        assert!(result.is_err());
        // State should NOT be cleared (emit_expired = false)
        // Note: The internal method still has the behavior based on emit_expired
        // With emit_expired = false, it shouldn't clear state
    }

    // ============================================================================
    // Get Quota Not Authenticated
    // ============================================================================

    #[tokio::test]
    async fn test_get_quota_not_authenticated() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service.get_quota().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_AUTHENTICATED");
    }

    // ============================================================================
    // Create Checkout/Portal Not Authenticated
    // ============================================================================

    #[tokio::test]
    async fn test_create_checkout_not_authenticated() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service.create_checkout_session("price_123").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_AUTHENTICATED");
    }

    #[tokio::test]
    async fn test_create_portal_not_authenticated() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service.create_portal_session().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_AUTHENTICATED");
    }

    #[tokio::test]
    async fn test_update_profile_not_authenticated() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let temp = tempdir().unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service
            .update_profile(Some("new@example.com"), None, None, None)
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_AUTHENTICATED");
    }

    // ============================================================================
    // Error Scenarios for Various Endpoints
    // ============================================================================

    #[tokio::test]
    async fn test_forgot_password_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/forgot-password"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Email not found"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.forgot_password("unknown@example.com").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_FOUND");
    }

    #[tokio::test]
    async fn test_reset_password_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/reset-password"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "message": "Invalid token"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service
            .reset_password("invalid_token", "new_password")
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "INVALID_REQUEST");
    }

    #[tokio::test]
    async fn test_get_prices_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/subscription/prices"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.get_prices().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "SERVER_ERROR");
    }

    #[tokio::test]
    async fn test_exchange_oauth_code_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/exchange"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "message": "Invalid code"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.exchange_oauth_code("invalid_code").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "INVALID_REQUEST");
    }

    // ============================================================================
    // Trait Implementation Tests
    // ============================================================================

    #[test]
    fn test_user_debug() {
        let user = User {
            id: 1,
            email: "test@example.com".to_string(),
            display_name: None,
            avatar_url: None,
        };

        let debug_str = format!("{:?}", user);
        assert!(debug_str.contains("User"));
        assert!(debug_str.contains("test@example.com"));
    }

    #[test]
    fn test_user_clone() {
        let user = User {
            id: 1,
            email: "test@example.com".to_string(),
            display_name: Some("Test".to_string()),
            avatar_url: None,
        };

        let cloned = user.clone();
        assert_eq!(cloned.id, user.id);
        assert_eq!(cloned.email, user.email);
    }

    #[test]
    fn test_subscription_debug() {
        let sub = Subscription {
            tier: "pro".to_string(),
            status: "active".to_string(),
            billing_interval: None,
            current_period_end: None,
        };

        let debug_str = format!("{:?}", sub);
        assert!(debug_str.contains("Subscription"));
        assert!(debug_str.contains("pro"));
    }

    #[test]
    fn test_quota_debug() {
        let quota = Quota {
            used: 100,
            limit: Some(1000),
            remaining: Some(900),
        };

        let debug_str = format!("{:?}", quota);
        assert!(debug_str.contains("Quota"));
    }

    #[test]
    fn test_price_debug() {
        let price = Price {
            id: "price_123".to_string(),
            product_id: "prod_456".to_string(),
            name: "Pro".to_string(),
            description: None,
            amount: 999,
            currency: "usd".to_string(),
            interval: "month".to_string(),
            features: None,
        };

        let debug_str = format!("{:?}", price);
        assert!(debug_str.contains("Price"));
    }

    #[test]
    fn test_auth_error_debug() {
        let error = AuthError {
            code: "TEST".to_string(),
            message: "Test error".to_string(),
        };

        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("AuthError"));
    }

    #[test]
    fn test_auth_state_partial_eq() {
        assert_eq!(AuthState::Initializing, AuthState::Initializing);
        assert_eq!(AuthState::Authenticated, AuthState::Authenticated);
        assert_eq!(AuthState::Unauthenticated, AuthState::Unauthenticated);
        assert_ne!(AuthState::Authenticated, AuthState::Unauthenticated);
    }

    #[test]
    fn test_auth_error_is_error() {
        let error = AuthError {
            code: "TEST".to_string(),
            message: "Test".to_string(),
        };

        // This tests that AuthError implements std::error::Error
        let _: &dyn std::error::Error = &error;
    }

    // ============================================================================
    // Update Profile with Password Change
    // ============================================================================

    #[tokio::test]
    async fn test_update_profile_with_password() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("PATCH"))
            .and(path("/api/user/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 1,
                "email": "test@example.com",
                "displayName": "Test User",
                "avatarUrl": null
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();

        let result = service
            .update_profile(None, None, Some("old_password"), Some("new_password"))
            .await;

        assert!(result.is_ok());
    }

    // ============================================================================
    // Default Base URL Test
    // ============================================================================

    #[test]
    fn test_default_base_url() {
        let temp = tempdir().unwrap();

        let service = AuthService::new(temp.path().to_path_buf(), None);

        // The service uses DEFAULT_BASE_URL when None is passed
        let oauth_url = service.get_oauth_url(None);
        assert!(oauth_url.starts_with("https://midlight.ai"));
    }

    // ============================================================================
    // Subscription Response Deserialization
    // ============================================================================

    #[test]
    fn test_subscription_response_deserialize() {
        let json = r#"{"subscription":{"tier":"free","status":"active","billingInterval":null,"currentPeriodEnd":null}}"#;
        let response: SubscriptionResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.subscription.tier, "free");
        assert_eq!(response.subscription.status, "active");
    }

    #[test]
    fn test_usage_response_deserialize() {
        let json = r#"{"quota":{"used":50,"limit":100,"remaining":50}}"#;
        let response: UsageResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.quota.used, 50);
        assert_eq!(response.quota.limit, Some(100));
    }

    #[test]
    fn test_prices_response_deserialize() {
        let json = r#"{"prices":[{"id":"price_1","productId":"prod_1","name":"Basic","description":null,"amount":499,"currency":"usd","interval":"month","features":null}]}"#;
        let response: PricesResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.prices.len(), 1);
        assert_eq!(response.prices[0].id, "price_1");
    }

    // ============================================================================
    // Login/Signup Request Serialization
    // ============================================================================

    #[test]
    fn test_login_request_serialize() {
        let request = LoginRequest {
            email: "test@example.com".to_string(),
            password: "secret".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"email\":\"test@example.com\""));
        assert!(json.contains("\"password\":\"secret\""));
    }

    #[test]
    fn test_signup_request_serialize() {
        let request = SignupRequest {
            email: "test@example.com".to_string(),
            password: "secret".to_string(),
            display_name: Some("Test User".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"displayName\":\"Test User\""));
    }

    #[test]
    fn test_exchange_code_request_serialize() {
        let request = ExchangeCodeRequest {
            code: "auth_code_123".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"code\":\"auth_code_123\""));
    }

    // ============================================================================
    // Quota with Unlimited Values
    // ============================================================================

    #[test]
    fn test_quota_unlimited() {
        let quota = Quota {
            used: 1000,
            limit: None,
            remaining: None,
        };

        let json = serde_json::to_string(&quota).unwrap();
        assert!(json.contains("\"used\":1000"));
        assert!(json.contains("\"limit\":null"));
        assert!(json.contains("\"remaining\":null"));
    }

    // ============================================================================
    // File System Error Tests
    // ============================================================================

    #[test]
    #[cfg(unix)]
    fn test_load_cookie_store_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let cookie_path = temp.path().join("cookies.json");

        // Create file with valid JSON content
        std::fs::write(&cookie_path, "[]").unwrap();
        // Remove all permissions
        std::fs::set_permissions(&cookie_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let store = AuthService::<RealTimeProvider>::load_cookie_store(temp.path());

        // Should return default store when permission denied
        assert!(store.iter_any().count() == 0);

        // Cleanup: restore permissions so tempdir can delete
        std::fs::set_permissions(&cookie_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_save_cookies_directory_creation_fails() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        // Create a read-only directory
        let readonly_dir = temp.path().join("readonly");
        std::fs::create_dir(&readonly_dir).unwrap();
        std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o444)).unwrap();

        // Service with path that requires creating nested directories inside read-only dir
        let service = AuthService::with_time_provider(
            readonly_dir.join("nested").join("deep"),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service.save_cookies();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "STORAGE_ERROR");
        assert!(error.message.contains("Failed to create directory"));

        // Cleanup
        std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_save_cookies_file_creation_fails() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        // Make directory read-only (can't create files)
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        let result = service.save_cookies();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "STORAGE_ERROR");
        assert!(error.message.contains("Failed to create cookie file"));

        // Cleanup
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_clear_cookies_delete_fails() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));

        let service = AuthService::with_time_provider(
            temp.path().to_path_buf(),
            Some("https://mock.test".to_string()),
            time_provider,
        );

        // First save cookies to create the file
        service.save_cookies().unwrap();
        let cookie_path = temp.path().join("cookies.json");
        assert!(cookie_path.exists());

        // Make directory read-only so file can't be deleted
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = service.clear_cookies();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "STORAGE_ERROR");
        assert!(error.message.contains("Failed to delete cookie file"));

        // Cleanup
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // ============================================================================
    // Network Error Tests
    // ============================================================================

    fn create_unreachable_service(
        time_provider: Arc<MockTimeProvider>,
    ) -> (AuthService<MockTimeProvider>, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();

        let service = AuthService::with_client_for_testing(
            temp.path().to_path_buf(),
            "http://127.0.0.1:1".to_string(), // Port 1 - connection refused
            client,
            time_provider,
        );
        (service, temp)
    }

    #[tokio::test]
    async fn test_login_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_signup_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        let result = service.signup("test@example.com", "password", None).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_refresh_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        let result = service.refresh_access_token().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_exchange_oauth_code_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        let result = service.exchange_oauth_code("code123").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_get_subscription_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        // Set up authenticated state
        service.set_tokens("test_token", 3600);
        service.set_auth_state(AuthState::Authenticated);

        let result = service.get_subscription().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_get_quota_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        service.set_tokens("test_token", 3600);
        service.set_auth_state(AuthState::Authenticated);

        let result = service.get_quota().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_get_prices_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        let result = service.get_prices().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_create_checkout_session_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        service.set_tokens("test_token", 3600);
        service.set_auth_state(AuthState::Authenticated);

        let result = service.create_checkout_session("price_123").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_create_portal_session_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        service.set_tokens("test_token", 3600);
        service.set_auth_state(AuthState::Authenticated);

        let result = service.create_portal_session().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_forgot_password_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        let result = service.forgot_password("test@example.com").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_reset_password_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        let result = service.reset_password("token", "newpass").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    #[tokio::test]
    async fn test_update_profile_network_error() {
        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let (service, _temp) = create_unreachable_service(time_provider);

        service.set_tokens("test_token", 3600);
        service.set_auth_state(AuthState::Authenticated);

        let result = service
            .update_profile(Some("new@example.com"), None, None, None)
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
    }

    // ============================================================================
    // Parse Error Tests
    // ============================================================================

    #[tokio::test]
    async fn test_login_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.login("test@example.com", "password").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_signup_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/signup"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.signup("test@example.com", "password", None).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_refresh_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{invalid}"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.refresh_access_token().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_exchange_oauth_code_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/exchange"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.exchange_oauth_code("code123").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_get_subscription_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/user/subscription"))
            .respond_with(ResponseTemplate::new(200).set_body_string("bad json"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.get_subscription().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_get_quota_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/user/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.get_quota().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_get_prices_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/subscription/prices"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        let result = service.get_prices().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_create_checkout_session_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/subscription/checkout"))
            .respond_with(ResponseTemplate::new(200).set_body_string("broken"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.create_checkout_session("price_123").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_create_portal_session_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/subscription/portal"))
            .respond_with(ResponseTemplate::new(200).set_body_string("nope"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.create_portal_session().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_update_profile_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("PATCH"))
            .and(path("/api/user/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_string("garbage"))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service
            .update_profile(Some("new@example.com"), None, None, None)
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "PARSE_ERROR");
    }

    // ============================================================================
    // Additional Edge Case Tests
    // ============================================================================

    #[tokio::test]
    async fn test_get_subscription_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/user/subscription"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "message": "Database error"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.get_subscription().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "SERVER_ERROR");
    }

    #[tokio::test]
    async fn test_get_quota_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/user/usage"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "Access denied"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.get_quota().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "AUTH_EXPIRED");
    }

    #[tokio::test]
    async fn test_create_checkout_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/subscription/checkout"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "message": "Invalid price ID"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.create_checkout_session("invalid_price").await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "INVALID_REQUEST");
    }

    #[tokio::test]
    async fn test_create_portal_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/subscription/portal"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "No subscription found"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service.create_portal_session().await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "NOT_FOUND");
    }

    #[tokio::test]
    async fn test_update_profile_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_auth_response()))
            .mount(&mock_server)
            .await;

        Mock::given(method("PATCH"))
            .and(path("/api/user/profile"))
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "message": "Email already in use"
            })))
            .mount(&mock_server)
            .await;

        let time_provider = Arc::new(MockTimeProvider::from_timestamp(1704067200));
        let service = create_test_service(&mock_server.uri(), time_provider);

        service.login("test@example.com", "password").await.unwrap();
        let result = service
            .update_profile(Some("taken@example.com"), None, None, None)
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code, "CONFLICT");
    }
}
