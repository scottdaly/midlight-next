// Error Reporter - Anonymous error reporting with PII sanitization
//
// Features:
// - Opt-in only (disabled by default)
// - Aggressive PII sanitization (file paths, emails, IPs, etc.)
// - Rate limiting (max 50 reports per session)
// - Fire-and-forget (no retry on failure)

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ============================================================================
// Types
// ============================================================================

/// Error report sent to the server
#[derive(Debug, Clone, Serialize)]
pub struct ErrorReport {
    pub schema_version: u32,
    pub category: String,
    pub error_type: String,
    pub message: String,
    pub sanitized: bool,
    pub app_version: String,
    pub platform: String,
    pub arch: String,
    pub os_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<HashMap<String, String>>,
    pub timestamp: String,
    pub session_id: String,
}

/// Error categories for grouping
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Import,
    Export,
    FileSystem,
    Editor,
    Llm,
    Auth,
    Recovery,
    Unknown,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::Import => write!(f, "import"),
            ErrorCategory::Export => write!(f, "export"),
            ErrorCategory::FileSystem => write!(f, "file_system"),
            ErrorCategory::Editor => write!(f, "editor"),
            ErrorCategory::Llm => write!(f, "llm"),
            ErrorCategory::Auth => write!(f, "auth"),
            ErrorCategory::Recovery => write!(f, "recovery"),
            ErrorCategory::Unknown => write!(f, "unknown"),
        }
    }
}

// ============================================================================
// PII Sanitization
// ============================================================================

/// Sanitize a message by removing potential PII
pub fn sanitize_message(message: &str) -> String {
    let mut result = message.to_string();

    // 1. Unix file paths: /Users/username/... or /home/username/...
    let unix_path = Regex::new(r"/(Users|home)/[^/\s]+").unwrap();
    result = unix_path.replace_all(&result, "/$1/[REDACTED]").to_string();

    // 2. Windows file paths: C:\Users\username\...
    let win_path = Regex::new(r"[A-Z]:\\Users\\[^\\\s]+").unwrap();
    result = win_path
        .replace_all(&result, "C:\\Users\\[REDACTED]")
        .to_string();

    // 3. Email addresses
    let email = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    result = email.replace_all(&result, "[EMAIL]").to_string();

    // 4. UUIDs (might identify users)
    let uuid_pattern =
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap();
    result = uuid_pattern.replace_all(&result, "[UUID]").to_string();

    // 5. IP addresses (IPv4)
    let ip = Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
    result = ip.replace_all(&result, "[IP]").to_string();

    // 6. Bearer tokens and API keys
    let bearer =
        Regex::new(r"Bearer\s+[A-Za-z0-9\-_]+\.?[A-Za-z0-9\-_]*\.?[A-Za-z0-9\-_]*").unwrap();
    result = bearer.replace_all(&result, "Bearer [TOKEN]").to_string();

    // 7. API keys (common patterns)
    let api_key = Regex::new(r"(sk-|pk-|api[-_]?key[=:]\s*)[A-Za-z0-9\-_]{20,}").unwrap();
    result = api_key.replace_all(&result, "$1[REDACTED]").to_string();

    // 8. Truncate to prevent accidental data exfiltration
    if result.len() > 1000 {
        result = format!("{}... [truncated]", &result[..1000]);
    }

    result
}

/// Sanitize context values
pub fn sanitize_context(context: &HashMap<String, String>) -> HashMap<String, String> {
    context
        .iter()
        .map(|(k, v)| (k.clone(), sanitize_message(v)))
        .collect()
}

// ============================================================================
// Error Reporter
// ============================================================================

/// Error reporter service
pub struct ErrorReporter {
    /// Random session ID (regenerated on each app launch)
    session_id: String,
    /// Whether error reporting is enabled (opt-in)
    enabled: AtomicBool,
    /// Number of reports sent this session
    reports_this_session: AtomicU32,
    /// Maximum reports per session
    max_reports_per_session: u32,
    /// API endpoint
    endpoint: String,
    /// HTTP client
    client: reqwest::Client,
    /// App version
    app_version: String,
}

impl ErrorReporter {
    /// Default maximum reports per session
    const DEFAULT_MAX_REPORTS: u32 = 50;

    /// API endpoint for error reports
    const DEFAULT_ENDPOINT: &'static str = "https://midlight.ai/api/error-report";

    /// Create a new error reporter
    pub fn new(app_version: &str) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            enabled: AtomicBool::new(false), // Opt-in, disabled by default
            reports_this_session: AtomicU32::new(0),
            max_reports_per_session: Self::DEFAULT_MAX_REPORTS,
            endpoint: Self::DEFAULT_ENDPOINT.to_string(),
            client: reqwest::Client::new(),
            app_version: app_version.to_string(),
        }
    }

    /// Enable or disable error reporting
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        if enabled {
            info!("Error reporting enabled");
        } else {
            info!("Error reporting disabled");
        }
    }

    /// Check if error reporting is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Get the number of reports sent this session
    pub fn reports_count(&self) -> u32 {
        self.reports_this_session.load(Ordering::SeqCst)
    }

    /// Get session ID (for debugging only - never expose to users)
    #[allow(dead_code)] // Useful for debugging
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Report an error (fire-and-forget)
    pub async fn report(
        &self,
        category: ErrorCategory,
        error_type: &str,
        message: &str,
        context: Option<HashMap<String, String>>,
    ) {
        // Check if enabled
        if !self.is_enabled() {
            debug!("Error reporting disabled, skipping report");
            return;
        }

        // Check rate limit
        let count = self.reports_this_session.fetch_add(1, Ordering::SeqCst);
        if count >= self.max_reports_per_session {
            warn!(
                "Error reporting rate limit reached ({}/{}), skipping",
                count, self.max_reports_per_session
            );
            self.reports_this_session.fetch_sub(1, Ordering::SeqCst); // Undo increment
            return;
        }

        // Build report with sanitization
        let report = ErrorReport {
            schema_version: 1,
            category: category.to_string(),
            error_type: error_type.to_string(),
            message: sanitize_message(message),
            sanitized: true,
            app_version: self.app_version.clone(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            os_version: get_os_version(),
            context: context.map(|c| sanitize_context(&c)),
            timestamp: chrono::Utc::now().to_rfc3339(),
            session_id: self.session_id.clone(),
        };

        // Send report (fire-and-forget)
        let endpoint = self.endpoint.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            match client.post(&endpoint).json(&report).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        debug!("Error report sent successfully");
                    } else {
                        debug!("Error report failed with status: {}", response.status());
                    }
                }
                Err(e) => {
                    debug!("Error report failed: {}", e);
                }
            }
        });
    }
}

impl Default for ErrorReporter {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Get OS version string
fn get_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("sw_vers").arg("-productVersion").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                return format!("macOS {}", version.trim());
            }
        }
        "macOS (unknown version)".to_string()
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("cmd").args(["/C", "ver"]).output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                return version.trim().to_string();
            }
        }
        "Windows (unknown version)".to_string()
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return line
                        .trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string();
                }
            }
        }
        "Linux (unknown distro)".to_string()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "Unknown OS".to_string()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_unix_paths() {
        let input = "Error at /Users/john/Documents/secret.txt";
        let output = sanitize_message(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("john"));
    }

    #[test]
    fn test_sanitize_windows_paths() {
        let input = "Error at C:\\Users\\john\\Documents\\secret.txt";
        let output = sanitize_message(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("john"));
    }

    #[test]
    fn test_sanitize_email() {
        let input = "User john.doe@example.com reported an error";
        let output = sanitize_message(input);
        assert!(output.contains("[EMAIL]"));
        assert!(!output.contains("john.doe@example.com"));
    }

    #[test]
    fn test_sanitize_uuid() {
        let input = "User 550e8400-e29b-41d4-a716-446655440000 not found";
        let output = sanitize_message(input);
        assert!(output.contains("[UUID]"));
        assert!(!output.contains("550e8400"));
    }

    #[test]
    fn test_sanitize_ip() {
        let input = "Connection from 192.168.1.100 failed";
        let output = sanitize_message(input);
        assert!(output.contains("[IP]"));
        assert!(!output.contains("192.168.1.100"));
    }

    #[test]
    fn test_sanitize_truncation() {
        let input = "x".repeat(2000);
        let output = sanitize_message(&input);
        assert!(output.len() < input.len());
        assert!(output.contains("[truncated]"));
    }

    #[test]
    fn test_reporter_disabled_by_default() {
        let reporter = ErrorReporter::new("1.0.0");
        assert!(!reporter.is_enabled());
    }

    #[test]
    fn test_reporter_enable_disable() {
        let reporter = ErrorReporter::new("1.0.0");
        reporter.set_enabled(true);
        assert!(reporter.is_enabled());
        reporter.set_enabled(false);
        assert!(!reporter.is_enabled());
    }
}
