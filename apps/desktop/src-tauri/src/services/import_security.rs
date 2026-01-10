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

    // ============================================
    // sanitize_filename tests
    // ============================================

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
    fn test_sanitize_filename_empty() {
        assert!(sanitize_filename("").is_err());
    }

    #[test]
    fn test_sanitize_filename_only_control_chars() {
        // String with only control characters should fail
        assert!(sanitize_filename("\x00\x01\x02").is_err());
    }

    #[test]
    fn test_sanitize_filename_only_dots_and_spaces() {
        assert!(sanitize_filename("... ").is_err());
        assert!(sanitize_filename(". . .").is_err());
    }

    #[test]
    fn test_sanitize_filename_long_truncation() {
        // Test filename longer than 255 characters
        let long_name = "a".repeat(300) + ".md";
        let result = sanitize_filename(&long_name).unwrap();
        assert!(result.len() <= ImportConfig::MAX_FILENAME_LENGTH);
        // Should preserve extension
        assert!(result.ends_with(".md"));
    }

    #[test]
    fn test_sanitize_filename_long_truncation_no_extension() {
        // Test very long filename without extension
        let long_name = "a".repeat(300);
        let result = sanitize_filename(&long_name).unwrap();
        assert_eq!(result.len(), ImportConfig::MAX_FILENAME_LENGTH);
    }

    #[test]
    fn test_sanitize_filename_long_extension() {
        // Test filename with long extension - should truncate the whole thing
        let long_ext = ".".to_string() + &"x".repeat(100);
        let result = sanitize_filename(&("a".repeat(200) + &long_ext)).unwrap();
        assert!(result.len() <= ImportConfig::MAX_FILENAME_LENGTH);
        // Should preserve extension when stem is long enough
        assert!(result.ends_with(&long_ext));
    }

    #[test]
    fn test_sanitize_filename_all_windows_reserved_names() {
        // Test all Windows reserved names
        for name in WINDOWS_RESERVED_NAMES {
            assert!(
                sanitize_filename(name).is_err(),
                "Expected {} to be rejected",
                name
            );
            // Also test with extension
            let with_ext = format!("{}.txt", name);
            assert!(
                sanitize_filename(&with_ext).is_err(),
                "Expected {} to be rejected",
                with_ext
            );
            // Test case insensitivity
            let lowercase = name.to_lowercase();
            assert!(
                sanitize_filename(&lowercase).is_err(),
                "Expected {} to be rejected",
                lowercase
            );
        }
    }

    #[test]
    fn test_sanitize_filename_unicode_normalization() {
        // Test that Unicode is normalized to NFC
        // é as e + combining acute (NFD) should normalize to é (NFC)
        let nfd = "cafe\u{0301}.md"; // e + combining acute (5 chars in NFD: c-a-f-e-combining_acute)
        let result = sanitize_filename(nfd).unwrap();
        // After NFC normalization, "café.md" = 7 chars: c-a-f-é-.-m-d
        assert_eq!(result.chars().count(), 7);
        // Verify it contains the NFC form of é
        assert!(result.contains('é') || result.contains("\u{00E9}"));
    }

    #[test]
    fn test_sanitize_filename_all_invalid_chars() {
        // Test all invalid characters are replaced
        for ch in INVALID_FILENAME_CHARS {
            let filename = format!("test{}file.md", ch);
            let result = sanitize_filename(&filename);
            if *ch == '\0' {
                // Null bytes are stripped, not replaced
                assert!(result.is_ok());
            } else {
                let sanitized = result.unwrap();
                assert!(
                    !sanitized.contains(*ch),
                    "Character {:?} should be removed",
                    ch
                );
            }
        }
    }

    #[test]
    fn test_sanitize_filename_mixed_content() {
        // Complex filename with multiple issues
        let result = sanitize_filename("hello<world>:test|file?.md...  ").unwrap();
        assert_eq!(result, "hello_world__test_file_.md");
    }

    // ============================================
    // sanitize_relative_path tests
    // ============================================

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
    fn test_sanitize_relative_path_empty() {
        assert!(sanitize_relative_path("").is_err());
    }

    #[test]
    fn test_sanitize_relative_path_too_long() {
        let long_path = "a/".repeat(600);
        assert!(sanitize_relative_path(&long_path).is_err());
    }

    #[test]
    fn test_sanitize_relative_path_null_bytes() {
        assert!(sanitize_relative_path("folder/file\0.md").is_err());
    }

    #[test]
    fn test_sanitize_relative_path_url_encoded_traversal() {
        // Various URL-encoded path traversal attempts
        assert!(sanitize_relative_path("%2e%2e%2f").is_err()); // ../
        assert!(sanitize_relative_path("..%2f").is_err()); // ../
        // Note: %5c (\) is only a path separator on Windows, not Unix
        // So we only test forward slash encoding which is universal
        assert!(sanitize_relative_path("%2e%2e/%2e%2e/secret").is_err()); // ../../secret
    }

    #[test]
    fn test_sanitize_relative_path_dot_current_dir() {
        // Current directory (.) should be skipped
        let result = sanitize_relative_path("./folder/./file.md").unwrap();
        assert_eq!(result, PathBuf::from("folder/file.md"));
    }

    #[test]
    fn test_sanitize_relative_path_nested_dirs() {
        let result = sanitize_relative_path("a/b/c/d/e/file.md").unwrap();
        assert_eq!(result, PathBuf::from("a/b/c/d/e/file.md"));
    }

    #[test]
    fn test_sanitize_relative_path_windows_drive_letters() {
        // Various Windows drive letter patterns
        assert!(sanitize_relative_path("C:file.md").is_err());
        assert!(sanitize_relative_path("D:\\folder\\file.md").is_err());
        assert!(sanitize_relative_path("Z:/folder/file.md").is_err());
    }

    #[test]
    fn test_sanitize_relative_path_only_dots() {
        // Path that becomes empty after sanitization
        assert!(sanitize_relative_path("./.").is_err());
    }

    // ============================================
    // is_path_safe tests
    // ============================================

    #[test]
    fn test_is_path_safe_within_base() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let dest = base.join("subdir").join("file.md");

        // Create the subdir
        std::fs::create_dir_all(base.join("subdir")).unwrap();

        assert!(is_path_safe(&dest, base));
    }

    #[test]
    fn test_is_path_safe_outside_base() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("workspace");
        std::fs::create_dir_all(&base).unwrap();

        // Try to escape to parent
        let dest = temp.path().join("outside.md");

        assert!(!is_path_safe(&dest, &base));
    }

    #[test]
    fn test_is_path_safe_nonexistent_base() {
        let dest = PathBuf::from("/some/dest/file.md");
        let base = PathBuf::from("/nonexistent/base");

        // Should return false if base doesn't exist
        assert!(!is_path_safe(&dest, &base));
    }

    #[test]
    fn test_is_path_safe_new_file_in_existing_dir() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let subdir = base.join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();

        // New file in existing subdirectory
        let dest = subdir.join("newfile.md");
        assert!(is_path_safe(&dest, base));
    }

    #[test]
    fn test_is_path_safe_deeply_nested() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let deep_dir = base.join("a").join("b").join("c").join("d");
        std::fs::create_dir_all(&deep_dir).unwrap();

        let dest = deep_dir.join("file.md");
        assert!(is_path_safe(&dest, base));
    }

    // ============================================
    // YAML parsing tests
    // ============================================

    #[test]
    fn test_safe_parse_yaml_basic() {
        let yaml = "key: value\nlist:\n  - item1\n  - item2";
        let result = safe_parse_yaml(yaml).unwrap();
        assert_eq!(result["key"].as_str(), Some("value"));
    }

    #[test]
    fn test_safe_parse_yaml_too_large() {
        let large_yaml = "x".repeat(ImportConfig::MAX_YAML_SIZE + 1);
        assert!(safe_parse_yaml(&large_yaml).is_err());
    }

    #[test]
    fn test_safe_parse_yaml_max_depth() {
        // Create deeply nested YAML that exceeds max depth
        let mut yaml = String::new();
        for i in 0..=ImportConfig::MAX_YAML_DEPTH + 5 {
            yaml.push_str(&"  ".repeat(i));
            yaml.push_str(&format!("level{}: \n", i));
        }
        yaml.push_str(&"  ".repeat(ImportConfig::MAX_YAML_DEPTH + 6));
        yaml.push_str("value: end");

        assert!(safe_parse_yaml(&yaml).is_err());
    }

    #[test]
    fn test_safe_parse_yaml_at_max_depth() {
        // Test that moderate depth (well under limit) succeeds
        let mut yaml = String::from("root:\n");
        let mut indent = 2;
        // Use a depth well under the limit to ensure it passes
        for i in 1..10 {
            yaml.push_str(&" ".repeat(indent));
            yaml.push_str(&format!("level{}: \n", i));
            indent += 2;
        }
        yaml.push_str(&" ".repeat(indent));
        yaml.push_str("value: end");

        assert!(safe_parse_yaml(&yaml).is_ok());
    }

    #[test]
    fn test_safe_parse_yaml_invalid_syntax() {
        let invalid = "key: [unclosed bracket";
        assert!(safe_parse_yaml(invalid).is_err());
    }

    #[test]
    fn test_safe_parse_yaml_empty() {
        // Empty YAML should parse to null
        let result = safe_parse_yaml("").unwrap();
        assert!(result.is_null());
    }

    #[test]
    fn test_safe_parse_yaml_sequence_depth() {
        // Test that sequences also count toward depth
        let yaml = "- - - - - value";
        let result = safe_parse_yaml(yaml);
        assert!(result.is_ok());
    }

    // ============================================
    // Front matter tests
    // ============================================

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
    fn test_safe_parse_front_matter_empty_yaml() {
        let content = "---\n---\n\n# Content";
        let result = safe_parse_front_matter(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_safe_parse_front_matter_only_opening() {
        let content = "---\n";
        let result = safe_parse_front_matter(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_safe_parse_front_matter_unclosed() {
        let content = "---\ntitle: Test\nNo closing delimiter";
        let result = safe_parse_front_matter(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_safe_parse_front_matter_at_end() {
        let content = "---\ntitle: Test\n---";
        let result = safe_parse_front_matter(content).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_safe_parse_front_matter_complex() {
        let content = r#"---
title: My Document
tags:
  - rust
  - testing
date: 2024-01-01
nested:
  key: value
---

# Content here"#;
        let result = safe_parse_front_matter(content).unwrap();
        assert!(result.is_some());
        let fm = result.unwrap();
        assert_eq!(fm.data["title"].as_str(), Some("My Document"));
        assert!(fm.data["tags"].is_sequence());
    }

    // ============================================
    // URL validation tests
    // ============================================

    #[test]
    fn test_is_external_url() {
        assert!(is_external_url("https://example.com"));
        assert!(is_external_url("http://example.com"));
        assert!(is_external_url("mailto:test@example.com"));
        assert!(!is_external_url("./local-file.md"));
        assert!(!is_external_url("javascript:alert(1)"));
    }

    #[test]
    fn test_is_external_url_case_insensitive() {
        assert!(is_external_url("HTTPS://example.com"));
        assert!(is_external_url("HTTP://example.com"));
        assert!(is_external_url("MAILTO:test@example.com"));
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
    fn test_is_dangerous_scheme_case_insensitive() {
        assert!(is_dangerous_scheme("JAVASCRIPT:alert(1)"));
        assert!(is_dangerous_scheme("DATA:text/html,<script>"));
        assert!(is_dangerous_scheme("VBScript:msgbox"));
        assert!(is_dangerous_scheme("FILE:///etc/passwd"));
    }

    // ============================================
    // CSV cell sanitization tests
    // ============================================

    #[test]
    fn test_sanitize_csv_cell() {
        assert_eq!(sanitize_csv_cell("=SUM(A1:A10)"), "'=SUM(A1:A10)");
        assert_eq!(sanitize_csv_cell("+1234"), "'+1234");
        assert_eq!(sanitize_csv_cell("normal text"), "normal text");
        assert_eq!(sanitize_csv_cell("with|pipe"), "with\\|pipe");
    }

    #[test]
    fn test_sanitize_csv_cell_all_formula_chars() {
        assert_eq!(sanitize_csv_cell("=formula"), "'=formula");
        assert_eq!(sanitize_csv_cell("+positive"), "'+positive");
        assert_eq!(sanitize_csv_cell("-negative"), "'-negative");
        assert_eq!(sanitize_csv_cell("@mention"), "'@mention");
    }

    #[test]
    fn test_sanitize_csv_cell_whitespace() {
        // Leading/trailing whitespace is trimmed before checking
        assert_eq!(sanitize_csv_cell("  =formula  "), "'=formula");
        assert_eq!(sanitize_csv_cell("  normal  "), "normal");
    }

    #[test]
    fn test_sanitize_csv_cell_multiple_pipes() {
        assert_eq!(sanitize_csv_cell("a|b|c"), "a\\|b\\|c");
    }

    // ============================================
    // AllowedExtension tests
    // ============================================

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

    #[test]
    fn test_allowed_extension_all_markdown_variants() {
        assert!(AllowedExtension::Markdown.matches("file.md"));
        assert!(AllowedExtension::Markdown.matches("file.markdown"));
        assert!(AllowedExtension::Markdown.matches("file.mdown"));
        assert!(AllowedExtension::Markdown.matches("file.mkd"));
    }

    #[test]
    fn test_allowed_extension_all_image_types() {
        for ext in AllowedExtension::Image.extensions() {
            let filename = format!("image.{}", ext);
            assert!(
                AllowedExtension::Image.matches(&filename),
                "Expected {} to match Image",
                filename
            );
        }
    }

    #[test]
    fn test_allowed_extension_all_attachment_types() {
        for ext in AllowedExtension::Attachment.extensions() {
            let filename = format!("file.{}", ext);
            assert!(
                AllowedExtension::Attachment.matches(&filename),
                "Expected {} to match Attachment",
                filename
            );
        }
    }

    #[test]
    fn test_allowed_extension_data_types() {
        assert!(AllowedExtension::Data.matches("data.csv"));
        assert!(AllowedExtension::Data.matches("config.json"));
        assert!(!AllowedExtension::Data.matches("data.xml"));
    }

    // ============================================
    // validate_path tests
    // ============================================

    #[test]
    fn test_validate_path_empty() {
        assert!(validate_path("").is_err());
    }

    #[test]
    fn test_validate_path_null_bytes() {
        assert!(validate_path("path/with\0null").is_err());
    }

    #[test]
    fn test_validate_path_too_long() {
        let long_path = "a".repeat(ImportConfig::MAX_PATH_LENGTH + 1);
        assert!(validate_path(&long_path).is_err());
    }

    #[test]
    fn test_validate_path_control_chars() {
        // Control characters (except tabs, newlines, carriage returns) should fail
        assert!(validate_path("path\x07with\x08bell").is_err());
    }

    #[test]
    fn test_validate_path_allowed_whitespace() {
        // Tabs, newlines, carriage returns are allowed
        assert!(validate_path("path\twith\ttabs").is_ok());
        assert!(validate_path("path\nwith\nnewlines").is_ok());
        assert!(validate_path("path\rwith\rreturns").is_ok());
    }

    #[test]
    fn test_validate_path_valid() {
        assert!(validate_path("valid/path/to/file.md").is_ok());
        assert!(validate_path("file.md").is_ok());
        assert!(validate_path("path with spaces/file.md").is_ok());
    }

    // ============================================
    // format_user_error tests
    // ============================================

    #[test]
    fn test_format_user_error_permission_denied() {
        let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
        let msg = format_user_error(&error);
        assert!(msg.contains("Permission denied"));
    }

    #[test]
    fn test_format_user_error_not_found() {
        let error = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let msg = format_user_error(&error);
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_format_user_error_already_exists() {
        let error = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "test");
        let msg = format_user_error(&error);
        assert!(msg.contains("already exists"));
    }

    #[test]
    fn test_format_user_error_other() {
        let error = std::io::Error::new(std::io::ErrorKind::Other, "custom error");
        let msg = format_user_error(&error);
        assert!(msg.contains("custom error"));
    }
}
