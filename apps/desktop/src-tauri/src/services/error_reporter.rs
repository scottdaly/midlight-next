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

    /// Create a new error reporter with custom endpoint (for testing)
    #[cfg(test)]
    pub fn with_endpoint(app_version: &str, endpoint: String) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            enabled: AtomicBool::new(false),
            reports_this_session: AtomicU32::new(0),
            max_reports_per_session: Self::DEFAULT_MAX_REPORTS,
            endpoint,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap(),
            app_version: app_version.to_string(),
        }
    }

    /// Create a new error reporter with custom rate limit (for testing)
    #[cfg(test)]
    pub fn with_rate_limit(app_version: &str, endpoint: String, max_reports: u32) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            enabled: AtomicBool::new(false),
            reports_this_session: AtomicU32::new(0),
            max_reports_per_session: max_reports,
            endpoint,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap(),
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

    /// Report an error and wait for the result (for testing)
    #[cfg(test)]
    pub async fn report_sync(
        &self,
        category: ErrorCategory,
        error_type: &str,
        message: &str,
        context: Option<HashMap<String, String>>,
    ) -> Option<reqwest::StatusCode> {
        // Check if enabled
        if !self.is_enabled() {
            return None;
        }

        // Check rate limit
        let count = self.reports_this_session.fetch_add(1, Ordering::SeqCst);
        if count >= self.max_reports_per_session {
            self.reports_this_session.fetch_sub(1, Ordering::SeqCst);
            return None;
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

        match self.client.post(&self.endpoint).json(&report).send().await {
            Ok(response) => Some(response.status()),
            Err(_) => None,
        }
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
    fn test_sanitize_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0";
        let output = sanitize_message(input);
        assert!(output.contains("[TOKEN]"));
        assert!(!output.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn test_sanitize_api_key() {
        let input = "Using api_key: sk-1234567890abcdefghijklmnopqrstuvwxyz";
        let output = sanitize_message(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("1234567890abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn test_sanitize_truncation() {
        let input = "x".repeat(2000);
        let output = sanitize_message(&input);
        assert!(output.len() < input.len());
        assert!(output.contains("[truncated]"));
    }

    #[test]
    fn test_sanitize_context() {
        let mut context = HashMap::new();
        context.insert("user_email".to_string(), "john@example.com".to_string());
        context.insert("path".to_string(), "/Users/john/file.txt".to_string());

        let sanitized = sanitize_context(&context);
        assert!(sanitized["user_email"].contains("[EMAIL]"));
        assert!(sanitized["path"].contains("[REDACTED]"));
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

    #[tokio::test]
    async fn test_report_when_disabled() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error-report"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0) // Should NOT be called
            .mount(&mock_server)
            .await;

        let reporter = ErrorReporter::with_endpoint(
            "1.0.0",
            format!("{}/api/error-report", mock_server.uri()),
        );
        // Reporter is disabled by default

        let result = reporter
            .report_sync(ErrorCategory::Unknown, "test", "test message", None)
            .await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_report_when_enabled() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error-report"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let reporter = ErrorReporter::with_endpoint(
            "1.0.0",
            format!("{}/api/error-report", mock_server.uri()),
        );
        reporter.set_enabled(true);

        let result = reporter
            .report_sync(ErrorCategory::Import, "ImportError", "File not found", None)
            .await;

        assert_eq!(result, Some(reqwest::StatusCode::OK));
        assert_eq!(reporter.reports_count(), 1);
    }

    #[tokio::test]
    async fn test_report_rate_limiting() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error-report"))
            .respond_with(ResponseTemplate::new(200))
            .expect(3) // Should only be called 3 times
            .mount(&mock_server)
            .await;

        let reporter = ErrorReporter::with_rate_limit(
            "1.0.0",
            format!("{}/api/error-report", mock_server.uri()),
            3, // Max 3 reports
        );
        reporter.set_enabled(true);

        // Send 5 reports, only 3 should go through
        for _ in 0..5 {
            reporter
                .report_sync(ErrorCategory::Unknown, "test", "message", None)
                .await;
        }

        assert_eq!(reporter.reports_count(), 3);
    }

    #[tokio::test]
    async fn test_report_with_context() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error-report"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let reporter = ErrorReporter::with_endpoint(
            "1.0.0",
            format!("{}/api/error-report", mock_server.uri()),
        );
        reporter.set_enabled(true);

        let mut context = HashMap::new();
        context.insert("file".to_string(), "test.md".to_string());
        context.insert("operation".to_string(), "import".to_string());

        let result = reporter
            .report_sync(
                ErrorCategory::Import,
                "ParseError",
                "Failed to parse file",
                Some(context),
            )
            .await;

        assert_eq!(result, Some(reqwest::StatusCode::OK));
    }

    #[tokio::test]
    async fn test_report_sanitizes_pii() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error-report"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let reporter = ErrorReporter::with_endpoint(
            "1.0.0",
            format!("{}/api/error-report", mock_server.uri()),
        );
        reporter.set_enabled(true);

        // Message contains PII
        let message = "Error for user@example.com at /Users/john/file.txt";

        let result = reporter
            .report_sync(ErrorCategory::FileSystem, "FileError", message, None)
            .await;

        assert_eq!(result, Some(reqwest::StatusCode::OK));
    }

    #[test]
    fn test_error_category_display() {
        assert_eq!(ErrorCategory::Import.to_string(), "import");
        assert_eq!(ErrorCategory::Export.to_string(), "export");
        assert_eq!(ErrorCategory::FileSystem.to_string(), "file_system");
        assert_eq!(ErrorCategory::Editor.to_string(), "editor");
        assert_eq!(ErrorCategory::Llm.to_string(), "llm");
        assert_eq!(ErrorCategory::Auth.to_string(), "auth");
        assert_eq!(ErrorCategory::Recovery.to_string(), "recovery");
        assert_eq!(ErrorCategory::Unknown.to_string(), "unknown");
    }

    // ============================================================================
    // Additional Tests
    // ============================================================================

    #[test]
    fn test_sanitize_home_path() {
        let input = "Error at /home/user/Documents/secret.txt";
        let output = sanitize_message(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("user"));
    }

    #[test]
    fn test_sanitize_multiple_emails() {
        let input = "From: alice@example.com, To: bob@company.org, CC: charlie@test.net";
        let output = sanitize_message(input);
        assert_eq!(output.matches("[EMAIL]").count(), 3);
        assert!(!output.contains("alice"));
        assert!(!output.contains("bob"));
        assert!(!output.contains("charlie"));
    }

    #[test]
    fn test_sanitize_multiple_ips() {
        let input = "Connections from 10.0.0.1, 192.168.1.1, and 172.16.0.1";
        let output = sanitize_message(input);
        assert_eq!(output.matches("[IP]").count(), 3);
        assert!(!output.contains("10.0.0.1"));
        assert!(!output.contains("192.168.1.1"));
        assert!(!output.contains("172.16.0.1"));
    }

    #[test]
    fn test_sanitize_pk_api_key() {
        let input = "Public key pk-abcdefghijklmnopqrstuvwxyz123456";
        let output = sanitize_message(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("abcdefghijklmnopqrstuvwxyz123456"));
    }

    #[test]
    fn test_sanitize_preserves_normal_text() {
        let input = "Error: File not found";
        let output = sanitize_message(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_sanitize_empty_string() {
        let output = sanitize_message("");
        assert_eq!(output, "");
    }

    #[test]
    fn test_sanitize_complex_message() {
        let input =
            "User john@test.com at /Users/john/Documents failed with error from 192.168.0.1";
        let output = sanitize_message(input);
        assert!(output.contains("[EMAIL]"));
        assert!(output.contains("[REDACTED]"));
        assert!(output.contains("[IP]"));
        assert!(!output.contains("john@test.com"));
        assert!(!output.contains("/Users/john"));
        assert!(!output.contains("192.168.0.1"));
    }

    #[test]
    fn test_reporter_session_id_unique() {
        let reporter1 = ErrorReporter::new("1.0.0");
        let reporter2 = ErrorReporter::new("1.0.0");
        assert_ne!(reporter1.session_id(), reporter2.session_id());
    }

    #[test]
    fn test_reporter_default() {
        let reporter = ErrorReporter::default();
        assert!(!reporter.is_enabled());
        assert_eq!(reporter.reports_count(), 0);
    }

    #[test]
    fn test_error_category_serialization() {
        let category = ErrorCategory::Import;
        let json = serde_json::to_string(&category).unwrap();
        assert_eq!(json, "\"import\"");

        let category = ErrorCategory::FileSystem;
        let json = serde_json::to_string(&category).unwrap();
        assert_eq!(json, "\"file_system\"");
    }

    #[test]
    fn test_error_category_deserialization() {
        let json = "\"import\"";
        let category: ErrorCategory = serde_json::from_str(json).unwrap();
        assert!(matches!(category, ErrorCategory::Import));

        let json = "\"file_system\"";
        let category: ErrorCategory = serde_json::from_str(json).unwrap();
        assert!(matches!(category, ErrorCategory::FileSystem));
    }

    #[test]
    fn test_error_report_serialization() {
        let report = ErrorReport {
            schema_version: 1,
            category: "import".to_string(),
            error_type: "ParseError".to_string(),
            message: "Test error".to_string(),
            sanitized: true,
            app_version: "1.0.0".to_string(),
            platform: "macos".to_string(),
            arch: "x86_64".to_string(),
            os_version: "macOS 14.0".to_string(),
            context: None,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            session_id: "test-session".to_string(),
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"category\":\"import\""));
        assert!(json.contains("\"error_type\":\"ParseError\""));
        assert!(json.contains("\"sanitized\":true"));
        // context should not be serialized when None
        assert!(!json.contains("context"));
    }

    #[test]
    fn test_error_report_with_context_serialization() {
        let mut context = HashMap::new();
        context.insert("key".to_string(), "value".to_string());

        let report = ErrorReport {
            schema_version: 1,
            category: "import".to_string(),
            error_type: "ParseError".to_string(),
            message: "Test error".to_string(),
            sanitized: true,
            app_version: "1.0.0".to_string(),
            platform: "macos".to_string(),
            arch: "x86_64".to_string(),
            os_version: "macOS 14.0".to_string(),
            context: Some(context),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            session_id: "test-session".to_string(),
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"context\":{\"key\":\"value\"}"));
    }

    #[tokio::test]
    async fn test_report_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error-report"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let reporter = ErrorReporter::with_endpoint(
            "1.0.0",
            format!("{}/api/error-report", mock_server.uri()),
        );
        reporter.set_enabled(true);

        let result = reporter
            .report_sync(ErrorCategory::Unknown, "test", "test message", None)
            .await;

        // Even on server error, we should get a status back
        assert_eq!(result, Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[test]
    fn test_sanitize_empty_context() {
        let context: HashMap<String, String> = HashMap::new();
        let sanitized = sanitize_context(&context);
        assert!(sanitized.is_empty());
    }

    #[test]
    fn test_sanitize_context_preserves_keys() {
        let mut context = HashMap::new();
        context.insert("operation".to_string(), "import".to_string());
        context.insert("file_count".to_string(), "10".to_string());

        let sanitized = sanitize_context(&context);
        assert_eq!(sanitized.get("operation"), Some(&"import".to_string()));
        assert_eq!(sanitized.get("file_count"), Some(&"10".to_string()));
    }

    #[tokio::test]
    async fn test_reports_count_increments() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error-report"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let reporter = ErrorReporter::with_endpoint(
            "1.0.0",
            format!("{}/api/error-report", mock_server.uri()),
        );
        reporter.set_enabled(true);

        assert_eq!(reporter.reports_count(), 0);

        reporter
            .report_sync(ErrorCategory::Unknown, "test", "message 1", None)
            .await;
        assert_eq!(reporter.reports_count(), 1);

        reporter
            .report_sync(ErrorCategory::Unknown, "test", "message 2", None)
            .await;
        assert_eq!(reporter.reports_count(), 2);
    }

    #[test]
    fn test_sanitize_uuid_lowercase() {
        let input = "User 550e8400-e29b-41d4-a716-446655440000 not found";
        let output = sanitize_message(input);
        assert!(output.contains("[UUID]"));
    }

    #[test]
    fn test_sanitize_uuid_uppercase() {
        let input = "User 550E8400-E29B-41D4-A716-446655440000 not found";
        let output = sanitize_message(input);
        assert!(output.contains("[UUID]"));
    }

    #[test]
    fn test_sanitize_truncation_exact_boundary() {
        // Test message exactly at 1000 chars - should not be truncated
        let input = "x".repeat(1000);
        let output = sanitize_message(&input);
        assert_eq!(output.len(), 1000);
        assert!(!output.contains("[truncated]"));
    }

    #[test]
    fn test_sanitize_truncation_just_over() {
        // Test message at 1001 chars - should be truncated
        let input = "x".repeat(1001);
        let output = sanitize_message(&input);
        assert!(output.contains("[truncated]"));
    }
}
