// Import security utilities
// Provides path sanitization, YAML safety, and validation for import operations

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

use super::error::ImportError;

/// Configuration constants for import security
#[allow(dead_code)] // Security config preserved for future use
pub struct ImportConfig;

#[allow(dead_code)] // Security constants preserved for future use
impl ImportConfig {
    /// Maximum content size for regex processing (10MB)
    pub const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;

    /// Maximum path length
    pub const MAX_PATH_LENGTH: usize = 1000;

    /// Maximum filename length
    pub const MAX_FILENAME_LENGTH: usize = 255;

    /// Maximum YAML size (1MB)
    pub const MAX_YAML_SIZE: usize = 1024 * 1024;

    /// Maximum YAML nesting depth
    pub const MAX_YAML_DEPTH: usize = 50;

    /// Parallel batch size for file processing
    pub const PARALLEL_BATCH_SIZE: usize = 10;

    /// Progress throttle interval in milliseconds
    pub const PROGRESS_THROTTLE_MS: u64 = 100;

    /// Large file threshold for checksum verification (10MB)
    pub const LARGE_FILE_THRESHOLD: u64 = 10 * 1024 * 1024;

    /// Disk space buffer percentage (10%)
    pub const DISK_SPACE_BUFFER: f64 = 0.1;
}

/// Allowed file extensions for import
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AllowedExtension {
    Markdown,
    Image,
    Attachment,
    Data,
}

impl AllowedExtension {
    /// Get the file extensions for this category
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            AllowedExtension::Markdown => &["md", "markdown", "mdown", "mkd"],
            AllowedExtension::Image => &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"],
            AllowedExtension::Attachment => &["pdf", "mp3", "mp4", "wav", "mov", "webm", "ogg"],
            AllowedExtension::Data => &["csv", "json"],
        }
    }

    /// Check if a filename has this extension type
    pub fn matches(&self, filename: &str) -> bool {
        let lower = filename.to_lowercase();
        self.extensions()
            .iter()
            .any(|ext| lower.ends_with(&format!(".{}", ext)))
    }

    /// Determine the extension type from a filename
    #[allow(dead_code)] // Useful for file type detection
    pub fn from_filename(filename: &str) -> Option<AllowedExtension> {
        if AllowedExtension::Markdown.matches(filename) {
            Some(AllowedExtension::Markdown)
        } else if AllowedExtension::Image.matches(filename) {
            Some(AllowedExtension::Image)
        } else if AllowedExtension::Attachment.matches(filename) {
            Some(AllowedExtension::Attachment)
        } else if AllowedExtension::Data.matches(filename) {
            Some(AllowedExtension::Data)
        } else {
            None
        }
    }
}

/// Windows reserved filenames that cannot be used
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Characters that are invalid in filenames across platforms
const INVALID_FILENAME_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0'];

/// Sanitize a filename for cross-platform safety
///
/// - Removes null bytes and control characters
/// - Handles Windows reserved names
/// - Removes trailing dots and spaces (Windows)
/// - Enforces max length (255 chars)
pub fn sanitize_filename(filename: &str) -> Result<String, ImportError> {
    if filename.is_empty() {
        return Err(ImportError::InvalidFilename(
            "Filename cannot be empty".into(),
        ));
    }

    // Normalize Unicode to NFC
    let normalized: String = filename.nfc().collect();

    // Remove null bytes and control characters
    let cleaned: String = normalized
        .chars()
        .filter(|c| !c.is_control() && *c != '\0')
        .collect();

    if cleaned.is_empty() {
        return Err(ImportError::InvalidFilename(
            "Filename contains only invalid characters".into(),
        ));
    }

    // Replace invalid filename characters with underscores
    let safe: String = cleaned
        .chars()
        .map(|c| {
            if INVALID_FILENAME_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();

    // Check for dangerous names (., ..)
    if safe == "." || safe == ".." {
        return Err(ImportError::InvalidFilename(format!(
            "Filename '{}' is not allowed",
            safe
        )));
    }

    // Check for Windows reserved names
    let name_without_ext = safe.split('.').next().unwrap_or(&safe).to_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&name_without_ext.as_str()) {
        return Err(ImportError::InvalidFilename(format!(
            "Filename '{}' uses a reserved Windows name",
            safe
        )));
    }

    // Remove trailing dots and spaces (Windows filesystem issue)
    let trimmed = safe.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return Err(ImportError::InvalidFilename(
            "Filename cannot consist only of dots and spaces".into(),
        ));
    }

    // Enforce max length
    if trimmed.len() > ImportConfig::MAX_FILENAME_LENGTH {
        // Truncate while preserving extension if possible
        let path = Path::new(trimmed);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let max_stem_len = ImportConfig::MAX_FILENAME_LENGTH - ext.len() - 1;
            if max_stem_len > 0 {
                let truncated_stem: String = stem.chars().take(max_stem_len).collect();
                return Ok(format!("{}.{}", truncated_stem, ext));
            }
        }
        let truncated: String = trimmed
            .chars()
            .take(ImportConfig::MAX_FILENAME_LENGTH)
            .collect();
        return Ok(truncated);
    }

    Ok(trimmed.to_string())
}

/// Sanitize a relative path for cross-platform safety
///
/// - Decodes URL encoding (prevents %2e%2e bypass)
/// - Normalizes Unicode (NFC)
/// - Rejects absolute paths
/// - Removes .. and . segments
/// - Validates each path component
pub fn sanitize_relative_path(relative_path: &str) -> Result<PathBuf, ImportError> {
    if relative_path.is_empty() {
        return Err(ImportError::InvalidPath("Path cannot be empty".into()));
    }

    // Check path length
    if relative_path.len() > ImportConfig::MAX_PATH_LENGTH {
        return Err(ImportError::InvalidPath(
            "Path exceeds maximum length".into(),
        ));
    }

    // Decode URL encoding to prevent %2e%2e/../ bypass
    let decoded = percent_decode_str(relative_path)
        .decode_utf8()
        .map_err(|_| ImportError::InvalidPath("Invalid UTF-8 in path".into()))?
        .to_string();

    // Normalize Unicode to NFC
    let normalized: String = decoded.nfc().collect();

    // Check for null bytes
    if normalized.contains('\0') {
        return Err(ImportError::InvalidPath("Path contains null bytes".into()));
    }

    // Reject absolute paths
    let path = Path::new(&normalized);
    if path.is_absolute() {
        return Err(ImportError::InvalidPath(
            "Absolute paths are not allowed".into(),
        ));
    }

    // Also check for Windows-style absolute paths
    if normalized.len() >= 2 {
        let bytes = normalized.as_bytes();
        if bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            return Err(ImportError::InvalidPath(
                "Absolute paths are not allowed".into(),
            ));
        }
    }

    // Build sanitized path, filtering out . and .. components
    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(name) => {
                let name_str = name.to_str().ok_or_else(|| {
                    ImportError::InvalidPath("Invalid UTF-8 in path component".into())
                })?;

                // Sanitize each component as a filename
                let safe_name = sanitize_filename(name_str)?;
                sanitized.push(safe_name);
            }
            std::path::Component::ParentDir => {
                // Reject .. - path traversal attempt
                return Err(ImportError::PathTraversal(
                    "Parent directory references are not allowed".into(),
                ));
            }
            std::path::Component::CurDir => {
                // Skip . - current directory references
                continue;
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(ImportError::InvalidPath(
                    "Absolute paths are not allowed".into(),
                ));
            }
        }
    }

    if sanitized.as_os_str().is_empty() {
        return Err(ImportError::InvalidPath(
            "Path resolves to empty after sanitization".into(),
        ));
    }

    Ok(sanitized)
}

/// Check if a destination path stays within the base directory
pub fn is_path_safe(dest_path: &Path, base_path: &Path) -> bool {
    // Canonicalize both paths if possible
    let canonical_base = match base_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };

    // For dest_path, we might need to check parent directories since the file
    // might not exist yet
    let mut check_path = dest_path.to_path_buf();

    // Walk up until we find an existing directory
    while !check_path.exists() {
        if let Some(parent) = check_path.parent() {
            check_path = parent.to_path_buf();
        } else {
            return false;
        }
    }

    let canonical_dest = match check_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };

    // Rebuild the full path with remaining components
    let remaining = dest_path.strip_prefix(&check_path).unwrap_or(Path::new(""));
    let full_canonical = canonical_dest.join(remaining);

    // Check if the destination starts with the base
    full_canonical.starts_with(&canonical_base)
}

/// Validate a path string for basic safety
#[allow(dead_code)] // Security validation preserved for future use
pub fn validate_path(input_path: &str) -> Result<(), ImportError> {
    if input_path.is_empty() {
        return Err(ImportError::InvalidPath("Path cannot be empty".into()));
    }

    // Check for null bytes
    if input_path.contains('\0') {
        return Err(ImportError::InvalidPath("Path contains null bytes".into()));
    }

    // Check length
    if input_path.len() > ImportConfig::MAX_PATH_LENGTH {
        return Err(ImportError::InvalidPath(
            "Path exceeds maximum length".into(),
        ));
    }

    // Check for control characters
    if input_path
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
    {
        return Err(ImportError::InvalidPath(
            "Path contains invalid control characters".into(),
        ));
    }

    Ok(())
}

/// Safely parse YAML content with size and depth limits
pub fn safe_parse_yaml(content: &str) -> Result<serde_yaml::Value, ImportError> {
    // Check size limit
    if content.len() > ImportConfig::MAX_YAML_SIZE {
        return Err(ImportError::YamlParse(format!(
            "YAML content exceeds maximum size of {} bytes",
            ImportConfig::MAX_YAML_SIZE
        )));
    }

    // Parse YAML
    let value: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| ImportError::YamlParse(e.to_string()))?;

    // Validate depth
    fn check_depth(value: &serde_yaml::Value, current_depth: usize) -> Result<(), ImportError> {
        if current_depth > ImportConfig::MAX_YAML_DEPTH {
            return Err(ImportError::YamlParse(format!(
                "YAML nesting exceeds maximum depth of {}",
                ImportConfig::MAX_YAML_DEPTH
            )));
        }

        match value {
            serde_yaml::Value::Mapping(map) => {
                for (_, v) in map {
                    check_depth(v, current_depth + 1)?;
                }
            }
            serde_yaml::Value::Sequence(seq) => {
                for v in seq {
                    check_depth(v, current_depth + 1)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    check_depth(&value, 0)?;

    Ok(value)
}

/// Parsed front matter from a markdown file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontMatter {
    pub raw: String,
    pub data: serde_yaml::Value,
}

/// Safely extract and parse YAML front matter from markdown content
pub fn safe_parse_front_matter(content: &str) -> Result<Option<FrontMatter>, ImportError> {
    // Check if content starts with ---
    if !content.starts_with("---") {
        return Ok(None);
    }

    // Find the closing ---
    let rest = &content[3..];
    let end_pos = rest.find("\n---");

    let yaml_content = match end_pos {
        Some(pos) => &rest[..pos],
        None => {
            // No closing ---, check if the entire content is just the opening
            if rest.trim().is_empty() {
                return Ok(None);
            }
            // Try to find --- on its own line
            if let Some(pos) = rest.find("\n---\n") {
                &rest[..pos]
            } else if rest.ends_with("\n---") {
                &rest[..rest.len() - 4]
            } else {
                return Ok(None);
            }
        }
    };

    // Trim leading newline if present
    let yaml_content = yaml_content.strip_prefix('\n').unwrap_or(yaml_content);

    if yaml_content.trim().is_empty() {
        return Ok(None);
    }

    let data = safe_parse_yaml(yaml_content)?;

    Ok(Some(FrontMatter {
        raw: yaml_content.to_string(),
        data,
    }))
}

/// Check if a URL is an external URL (http, https, mailto)
#[allow(dead_code)] // Security check preserved for future use
pub fn is_external_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Check if a URL uses a dangerous scheme
#[allow(dead_code)] // Security check preserved for future use
pub fn is_dangerous_scheme(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("file:")
}

/// Sanitize a CSV cell to prevent formula injection
pub fn sanitize_csv_cell(cell: &str) -> String {
    let trimmed = cell.trim();

    // Check for formula-starting characters
    if trimmed.starts_with('=')
        || trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.starts_with('@')
    {
        // Prefix with single quote to prevent formula execution
        format!("'{}", trimmed)
    } else {
        // Escape pipe characters for Markdown tables
        trimmed.replace('|', "\\|")
    }
}

/// Format a system error into a user-friendly message
#[allow(dead_code)] // Error formatting preserved for future use
pub fn format_user_error(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => {
            "Permission denied. Please check file permissions.".into()
        }
        std::io::ErrorKind::NotFound => "File or directory not found.".into(),
        std::io::ErrorKind::AlreadyExists => "File or directory already exists.".into(),
        std::io::ErrorKind::InvalidInput => "Invalid input provided.".into(),
        std::io::ErrorKind::InvalidData => "File contains invalid data.".into(),
        std::io::ErrorKind::TimedOut => "Operation timed out.".into(),
        std::io::ErrorKind::Interrupted => "Operation was interrupted.".into(),
        std::io::ErrorKind::OutOfMemory => "Out of memory.".into(),
        _ => {
            // Check for ENOSPC (no space left on device)
            if error.raw_os_error() == Some(28) {
                "Insufficient disk space.".into()
            } else {
                format!("An error occurred: {}", error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_basic() {
        assert_eq!(sanitize_filename("hello.md").unwrap(), "hello.md");
        assert_eq!(
            sanitize_filename("Hello World.md").unwrap(),
            "Hello World.md"
        );
    }

    #[test]
    fn test_sanitize_filename_invalid_chars() {
        assert_eq!(
            sanitize_filename("hello<world>.md").unwrap(),
            "hello_world_.md"
        );
        assert_eq!(sanitize_filename("test:file.md").unwrap(), "test_file.md");
    }

    #[test]
    fn test_sanitize_filename_dangerous() {
        assert!(sanitize_filename(".").is_err());
        assert!(sanitize_filename("..").is_err());
        assert!(sanitize_filename("CON").is_err());
        assert!(sanitize_filename("PRN.txt").is_err());
    }

    #[test]
    fn test_sanitize_filename_trailing() {
        assert_eq!(sanitize_filename("hello...").unwrap(), "hello");
        assert_eq!(sanitize_filename("hello   ").unwrap(), "hello");
    }

    #[test]
    fn test_sanitize_relative_path_basic() {
        let result = sanitize_relative_path("folder/file.md").unwrap();
        assert_eq!(result, PathBuf::from("folder/file.md"));
    }

    #[test]
    fn test_sanitize_relative_path_traversal() {
        assert!(sanitize_relative_path("../secret.md").is_err());
        assert!(sanitize_relative_path("folder/../secret.md").is_err());
        assert!(sanitize_relative_path("%2e%2e/secret.md").is_err());
    }

    #[test]
    fn test_sanitize_relative_path_absolute() {
        assert!(sanitize_relative_path("/etc/passwd").is_err());
        assert!(sanitize_relative_path("C:\\Windows\\System32").is_err());
    }

    #[test]
    fn test_safe_parse_front_matter() {
        let content = "---\ntitle: Hello\nauthor: Test\n---\n\n# Content";
        let result = safe_parse_front_matter(content).unwrap();
        assert!(result.is_some());
        let fm = result.unwrap();
        assert!(fm.data["title"].as_str() == Some("Hello"));
    }

    #[test]
    fn test_safe_parse_front_matter_none() {
        let content = "# No front matter\n\nJust content";
        let result = safe_parse_front_matter(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_is_external_url() {
        assert!(is_external_url("https://example.com"));
        assert!(is_external_url("http://example.com"));
        assert!(is_external_url("mailto:test@example.com"));
        assert!(!is_external_url("./local-file.md"));
        assert!(!is_external_url("javascript:alert(1)"));
    }

    #[test]
    fn test_is_dangerous_scheme() {
        assert!(is_dangerous_scheme("javascript:alert(1)"));
        assert!(is_dangerous_scheme("data:text/html,<script>"));
        assert!(is_dangerous_scheme("vbscript:msgbox"));
        assert!(is_dangerous_scheme("file:///etc/passwd"));
        assert!(!is_dangerous_scheme("https://example.com"));
    }

    #[test]
    fn test_sanitize_csv_cell() {
        assert_eq!(sanitize_csv_cell("=SUM(A1:A10)"), "'=SUM(A1:A10)");
        assert_eq!(sanitize_csv_cell("+1234"), "'+1234");
        assert_eq!(sanitize_csv_cell("normal text"), "normal text");
        assert_eq!(sanitize_csv_cell("with|pipe"), "with\\|pipe");
    }

    #[test]
    fn test_allowed_extension() {
        assert!(AllowedExtension::Markdown.matches("test.md"));
        assert!(AllowedExtension::Markdown.matches("TEST.MD"));
        assert!(AllowedExtension::Image.matches("photo.jpg"));
        assert!(AllowedExtension::Image.matches("image.PNG"));
        assert!(!AllowedExtension::Markdown.matches("test.txt"));

        assert_eq!(
            AllowedExtension::from_filename("test.md"),
            Some(AllowedExtension::Markdown)
        );
        assert_eq!(
            AllowedExtension::from_filename("image.png"),
            Some(AllowedExtension::Image)
        );
        assert_eq!(AllowedExtension::from_filename("test.exe"), None);
    }
}
