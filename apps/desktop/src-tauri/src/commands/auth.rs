// Auth Commands - Tauri IPC handlers for authentication

use crate::services::auth_service::{CheckoutSession, PortalSession, Price, Quota, Subscription, User, AUTH_SERVICE};
use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, error, info};

// ============================================================================
// Event Types
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStateChangedEvent {
    pub state: String,
    pub user: Option<User>,
}

// ============================================================================
// Commands
// ============================================================================

/// Initialize auth service and attempt silent refresh
#[tauri::command]
pub async fn auth_init() -> Result<String, String> {
    debug!("auth_init command");

    AUTH_SERVICE
        .init()
        .await
        .map(|state| state.to_string())
        .map_err(|e| e.to_string())
}

/// Email/password signup
#[tauri::command]
pub async fn auth_signup(
    email: String,
    password: String,
    display_name: Option<String>,
) -> Result<User, String> {
    debug!("auth_signup command: {}", email);

    AUTH_SERVICE
        .signup(&email, &password, display_name.as_deref())
        .await
        .map(|response| response.user)
        .map_err(|e| e.to_string())
}

/// Email/password login
#[tauri::command]
pub async fn auth_login(email: String, password: String) -> Result<User, String> {
    debug!("auth_login command: {}", email);

    AUTH_SERVICE
        .login(&email, &password)
        .await
        .map(|response| response.user)
        .map_err(|e| e.to_string())
}

/// Logout
#[tauri::command]
pub async fn auth_logout() -> Result<(), String> {
    debug!("auth_logout command");

    AUTH_SERVICE.logout().await.map_err(|e| e.to_string())
}

/// Start Google OAuth flow with local callback server
#[tauri::command]
pub async fn auth_login_with_google(app: AppHandle) -> Result<(), String> {
    debug!("auth_login_with_google command");

    // Start a TCP listener on a random available port
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| {
        error!("Failed to bind TCP listener: {}", e);
        format!("Failed to start OAuth callback server: {}", e)
    })?;

    let port = listener.local_addr().map_err(|e| {
        error!("Failed to get local address: {}", e);
        format!("Failed to get callback port: {}", e)
    })?.port();

    info!("OAuth callback server listening on port {}", port);

    // Build OAuth URL with callback port
    let url = AUTH_SERVICE.get_oauth_url(Some(port));

    // Open browser
    if let Err(e) = open::that(&url) {
        error!("Failed to open OAuth URL: {}", e);
        return Err(format!("Failed to open browser: {}", e));
    }

    // Handle callback in background
    tauri::async_runtime::spawn(async move {
        // Wait for a connection (with timeout)
        listener.set_nonblocking(false).ok();

        match listener.accept() {
            Ok((mut stream, _)) => {
                // Read the HTTP request
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();

                if reader.read_line(&mut request_line).is_err() {
                    error!("Failed to read OAuth callback request");
                    return;
                }

                debug!("OAuth callback request: {}", request_line.trim());

                // Parse the code from the request URL
                // Format: GET /auth/callback?code=XXX HTTP/1.1
                let code = request_line
                    .split_whitespace()
                    .nth(1) // Get the path
                    .and_then(|path| {
                        path.split('?')
                            .nth(1) // Get query string
                            .and_then(|query| {
                                query.split('&')
                                    .find(|param| param.starts_with("code="))
                                    .map(|param| param.trim_start_matches("code=").to_string())
                            })
                    });

                // Send response to browser
                let response_body = if code.is_some() {
                    r#"<!DOCTYPE html>
<html>
<head>
    <title>Sign In Successful</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #f5f5f5; }
        .container { text-align: center; padding: 40px; background: white; border-radius: 12px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        h1 { color: #333; margin-bottom: 8px; }
        p { color: #666; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Sign In Successful!</h1>
        <p>You can close this window and return to Midlight.</p>
    </div>
</body>
</html>"#
                } else {
                    r#"<!DOCTYPE html>
<html>
<head>
    <title>Sign In Failed</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #f5f5f5; }
        .container { text-align: center; padding: 40px; background: white; border-radius: 12px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        h1 { color: #c00; margin-bottom: 8px; }
        p { color: #666; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Sign In Failed</h1>
        <p>Something went wrong. Please try again.</p>
    </div>
</body>
</html>"#
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );

                if let Err(e) = stream.write_all(response.as_bytes()) {
                    error!("Failed to send OAuth callback response: {}", e);
                }

                // Exchange code for tokens
                if let Some(code) = code {
                    match AUTH_SERVICE.exchange_oauth_code(&code).await {
                        Ok(response) => {
                            info!("OAuth exchange successful");

                            // Emit auth state changed event
                            let event = AuthStateChangedEvent {
                                state: "authenticated".to_string(),
                                user: Some(response.user),
                            };

                            if let Err(e) = app.emit("auth:state-changed", &event) {
                                error!("Failed to emit auth state changed event: {}", e);
                            }

                            // Bring the app window to focus
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.set_focus();
                            }
                        }
                        Err(e) => {
                            error!("OAuth code exchange failed: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to accept OAuth callback connection: {}", e);
            }
        }
    });

    Ok(())
}

/// Handle OAuth callback (called when deep link received)
#[tauri::command]
pub async fn auth_handle_oauth_callback(app: AppHandle, code: String) -> Result<User, String> {
    debug!("auth_handle_oauth_callback command");

    let response = AUTH_SERVICE
        .exchange_oauth_code(&code)
        .await
        .map_err(|e| e.to_string())?;

    // Emit auth state changed event
    let event = AuthStateChangedEvent {
        state: "authenticated".to_string(),
        user: Some(response.user.clone()),
    };

    if let Err(e) = app.emit("auth:state-changed", &event) {
        error!("Failed to emit auth state changed event: {}", e);
    }

    Ok(response.user)
}

/// Get current user
#[tauri::command]
pub async fn auth_get_user() -> Result<Option<User>, String> {
    debug!("auth_get_user command");
    Ok(AUTH_SERVICE.get_user())
}

/// Get subscription info
#[tauri::command]
pub async fn auth_get_subscription() -> Result<Subscription, String> {
    debug!("auth_get_subscription command");

    AUTH_SERVICE
        .get_subscription()
        .await
        .map_err(|e| e.to_string())
}

/// Get quota info
#[tauri::command]
pub async fn auth_get_quota() -> Result<Quota, String> {
    debug!("auth_get_quota command");

    AUTH_SERVICE.get_quota().await.map_err(|e| e.to_string())
}

/// Check if user is authenticated
#[tauri::command]
pub async fn auth_is_authenticated() -> bool {
    debug!("auth_is_authenticated command");
    AUTH_SERVICE.is_authenticated()
}

/// Get current auth state
#[tauri::command]
pub async fn auth_get_state() -> String {
    debug!("auth_get_state command");
    AUTH_SERVICE.get_auth_state().to_string()
}

/// Get current access token (for LLM requests)
#[tauri::command]
pub async fn auth_get_access_token() -> Option<String> {
    debug!("auth_get_access_token command");
    AUTH_SERVICE.get_access_token().await
}

/// Request password reset email
#[tauri::command]
pub async fn auth_forgot_password(email: String) -> Result<(), String> {
    debug!("auth_forgot_password command: {}", email);

    AUTH_SERVICE
        .forgot_password(&email)
        .await
        .map_err(|e| e.to_string())
}

/// Reset password with token
#[tauri::command]
pub async fn auth_reset_password(token: String, new_password: String) -> Result<(), String> {
    debug!("auth_reset_password command");

    AUTH_SERVICE
        .reset_password(&token, &new_password)
        .await
        .map_err(|e| e.to_string())
}

/// Update user profile
#[tauri::command]
pub async fn auth_update_profile(
    email: Option<String>,
    display_name: Option<String>,
    current_password: Option<String>,
    new_password: Option<String>,
) -> Result<User, String> {
    debug!("auth_update_profile command");

    AUTH_SERVICE
        .update_profile(
            email.as_deref(),
            display_name.as_deref(),
            current_password.as_deref(),
            new_password.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// Subscription Commands
// ============================================================================

/// Get available subscription prices
#[tauri::command]
pub async fn subscription_get_prices() -> Result<Vec<Price>, String> {
    debug!("subscription_get_prices command");

    AUTH_SERVICE
        .get_prices()
        .await
        .map_err(|e| e.to_string())
}

/// Create Stripe checkout session and open in browser
#[tauri::command]
pub async fn subscription_create_checkout(price_id: String) -> Result<CheckoutSession, String> {
    debug!("subscription_create_checkout command: {}", price_id);

    let session = AUTH_SERVICE
        .create_checkout_session(&price_id)
        .await
        .map_err(|e| e.to_string())?;

    // Open checkout URL in browser
    if let Err(e) = open::that(&session.url) {
        error!("Failed to open checkout URL: {}", e);
        // Don't fail the command, return the URL so frontend can handle it
    }

    Ok(session)
}

/// Create Stripe billing portal session and open in browser
#[tauri::command]
pub async fn subscription_create_portal() -> Result<PortalSession, String> {
    debug!("subscription_create_portal command");

    let session = AUTH_SERVICE
        .create_portal_session()
        .await
        .map_err(|e| e.to_string())?;

    // Open portal URL in browser
    if let Err(e) = open::that(&session.url) {
        error!("Failed to open portal URL: {}", e);
        // Don't fail the command, return the URL so frontend can handle it
    }

    Ok(session)
}
