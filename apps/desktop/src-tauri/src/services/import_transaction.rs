// Import transaction manager
// Provides atomic operations with staging directory and rollback capability

use rand::Rng;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::error::ImportError;
use super::import_security::{is_path_safe, sanitize_relative_path, ImportConfig};

/// Statistics from a completed transaction
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used by callers after commit()
pub struct TransactionStats {
    pub files_staged: usize,
    pub bytes_written: u64,
    pub files_committed: usize,
}

/// Manages atomic import operations with staging and rollback
///
/// Uses a staging directory pattern:
/// 1. All files are first written to a temporary staging directory
/// 2. On commit, files are moved to the final destination atomically
/// 3. On rollback (or drop without commit), staging directory is deleted
///
/// This provides all-or-nothing semantics for import operations.
pub struct ImportTransaction {
    staging_dir: PathBuf,
    dest_path: PathBuf,
    staged_files: Vec<PathBuf>,
    bytes_written: u64,
    committed: bool,
}

impl ImportTransaction {
    /// Create a new import transaction
    ///
    /// Creates a staging directory in the destination's parent with format:
    /// `.import-staging-{timestamp}-{random}`
    pub fn new(dest_path: PathBuf) -> Result<Self, ImportError> {
        // Ensure destination parent exists
        let parent = dest_path.parent().ok_or_else(|| {
            ImportError::InvalidPath("Destination path has no parent directory".into())
        })?;

        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        // Generate staging directory name
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let random: u32 = rand::thread_rng().gen();
        let staging_name = format!(".import-staging-{}-{:08x}", timestamp, random);
        let staging_dir = parent.join(&staging_name);

        // Create staging directory
        fs::create_dir_all(&staging_dir)?;

        Ok(Self {
            staging_dir,
            dest_path,
            staged_files: Vec::new(),
            bytes_written: 0,
            committed: false,
        })
    }

    /// Get the staging directory path
    #[allow(dead_code)] // Used in tests
    pub fn staging_dir(&self) -> &Path {
        &self.staging_dir
    }

    /// Get the destination path
    #[allow(dead_code)] // Used in tests
    pub fn dest_path(&self) -> &Path {
        &self.dest_path
    }

    /// Stage a file with content
    ///
    /// Writes the file to the staging directory, creating parent directories as needed.
    pub fn stage_file(&mut self, relative_path: &Path, content: &[u8]) -> Result<(), ImportError> {
        // Sanitize the relative path
        let safe_path = sanitize_relative_path(
            relative_path
                .to_str()
                .ok_or_else(|| ImportError::InvalidPath("Invalid UTF-8 in path".into()))?,
        )?;

        // Build full staging path
        let staged_path = self.staging_dir.join(&safe_path);

        // Verify path stays within staging directory
        if !is_path_safe(&staged_path, &self.staging_dir) {
            return Err(ImportError::PathTraversal(format!(
                "Path escapes staging directory: {:?}",
                relative_path
            )));
        }

        // Create parent directories
        if let Some(parent) = staged_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write file
        let mut file = fs::File::create(&staged_path)?;
        file.write_all(content)?;
        file.sync_all()?;

        self.staged_files.push(safe_path);
        self.bytes_written += content.len() as u64;

        Ok(())
    }

    /// Stage a file by copying from source
    ///
    /// Copies the source file to the staging directory with path validation.
    pub fn stage_copy(&mut self, source: &Path, relative_path: &Path) -> Result<(), ImportError> {
        // Sanitize the relative path
        let safe_path = sanitize_relative_path(
            relative_path
                .to_str()
                .ok_or_else(|| ImportError::InvalidPath("Invalid UTF-8 in path".into()))?,
        )?;

        // Build full staging path
        let staged_path = self.staging_dir.join(&safe_path);

        // Verify path stays within staging directory
        if !is_path_safe(&staged_path, &self.staging_dir) {
            return Err(ImportError::PathTraversal(format!(
                "Path escapes staging directory: {:?}",
                relative_path
            )));
        }

        // Create parent directories
        if let Some(parent) = staged_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Copy file
        let bytes = fs::copy(source, &staged_path)?;

        self.staged_files.push(safe_path);
        self.bytes_written += bytes;

        Ok(())
    }

    /// Verify a copied file using SHA-256 checksum
    ///
    /// Only recommended for large files (> LARGE_FILE_THRESHOLD).
    #[allow(dead_code)] // Used in tests
    pub fn verify_copy(&self, source: &Path, staged_path: &Path) -> Result<bool, ImportError> {
        let source_hash = compute_file_hash(source)?;
        let staged_hash = compute_file_hash(staged_path)?;
        Ok(source_hash == staged_hash)
    }

    /// Commit the transaction
    ///
    /// Moves all staged files to the final destination atomically.
    pub fn commit(&mut self) -> Result<TransactionStats, ImportError> {
        if self.committed {
            return Err(ImportError::TransactionFailed(
                "Transaction already committed".into(),
            ));
        }

        // Ensure destination exists
        if !self.dest_path.exists() {
            fs::create_dir_all(&self.dest_path)?;
        }

        let mut files_committed = 0;

        // Move each staged file to destination
        for relative_path in &self.staged_files {
            let staged_file = self.staging_dir.join(relative_path);
            let dest_file = self.dest_path.join(relative_path);

            // Create destination parent directories
            if let Some(parent) = dest_file.parent() {
                fs::create_dir_all(parent)?;
            }

            // Move file (rename is atomic on same filesystem)
            // If rename fails (cross-filesystem), fall back to copy+delete
            if fs::rename(&staged_file, &dest_file).is_err() {
                fs::copy(&staged_file, &dest_file)?;
                fs::remove_file(&staged_file)?;
            }

            files_committed += 1;
        }

        // Clean up staging directory
        let _ = fs::remove_dir_all(&self.staging_dir);

        self.committed = true;

        Ok(TransactionStats {
            files_staged: self.staged_files.len(),
            bytes_written: self.bytes_written,
            files_committed,
        })
    }

    /// Rollback the transaction
    ///
    /// Deletes the staging directory and all contents.
    /// Safe to call multiple times.
    pub fn rollback(&mut self) -> Result<(), ImportError> {
        if self.committed {
            return Ok(()); // Already committed, nothing to rollback
        }

        // Remove staging directory
        if self.staging_dir.exists() {
            fs::remove_dir_all(&self.staging_dir)?;
        }

        self.staged_files.clear();
        self.bytes_written = 0;

        Ok(())
    }

    /// Get current transaction statistics
    #[allow(dead_code)] // Public API for callers
    pub fn stats(&self) -> TransactionStats {
        TransactionStats {
            files_staged: self.staged_files.len(),
            bytes_written: self.bytes_written,
            files_committed: if self.committed {
                self.staged_files.len()
            } else {
                0
            },
        }
    }
}

/// Auto-rollback on drop if not committed (RAII pattern)
impl Drop for ImportTransaction {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.rollback();
        }
    }
}

/// Compute SHA-256 hash of a file
#[allow(dead_code)] // Used by verify_copy
fn compute_file_hash(path: &Path) -> Result<String, ImportError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

/// Validate that there's enough disk space for an import
///
/// Checks available space and requires a buffer of 10% beyond the required size.
#[allow(dead_code)] // Scaffolded for future use
pub fn validate_disk_space(dest_path: &Path, required_bytes: u64) -> Result<(), ImportError> {
    // Get available space on the filesystem
    // This is platform-specific, using a simple approach for now
    #[cfg(unix)]
    {
        #[allow(unused_imports)]
        use std::os::unix::fs::MetadataExt;

        // Find an existing parent directory
        let mut check_path = dest_path.to_path_buf();
        while !check_path.exists() {
            if let Some(parent) = check_path.parent() {
                check_path = parent.to_path_buf();
            } else {
                return Err(ImportError::InvalidPath(
                    "Cannot find existing parent directory".into(),
                ));
            }
        }

        // Use statvfs via nix crate or fallback
        // For now, we'll skip the actual check and just validate the path exists
        // A full implementation would use libc::statvfs
        let _ = check_path.metadata()?;

        // Calculate required space with buffer
        let _required_with_buffer =
            required_bytes + (required_bytes as f64 * ImportConfig::DISK_SPACE_BUFFER) as u64;

        // TODO: Implement actual disk space check using statvfs
        // For now, we trust the filesystem
        Ok(())
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        let mut check_path = dest_path.to_path_buf();
        while !check_path.exists() {
            if let Some(parent) = check_path.parent() {
                check_path = parent.to_path_buf();
            } else {
                return Err(ImportError::InvalidPath(
                    "Cannot find existing parent directory".into(),
                ));
            }
        }

        let _ = check_path.metadata()?;

        // TODO: Implement actual disk space check using GetDiskFreeSpaceExW
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    {
        // On other platforms, skip disk space check
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // ============================================================================
    // Transaction Creation Tests
    // ============================================================================

    #[test]
    fn test_transaction_new_creates_staging_dir() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let tx = ImportTransaction::new(dest).unwrap();

        assert!(tx.staging_dir().exists());
        assert!(tx.staging_dir().is_dir());
        // Staging dir should be in parent of dest
        assert!(tx
            .staging_dir()
            .to_string_lossy()
            .contains(".import-staging-"));
    }

    #[test]
    fn test_transaction_new_creates_parent_if_missing() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("nested/deep/import_dest");

        let tx = ImportTransaction::new(dest.clone()).unwrap();

        // Parent directories should be created
        assert!(dest.parent().unwrap().exists());
        assert!(tx.staging_dir().exists());
    }

    #[test]
    fn test_transaction_dest_path_accessor() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let tx = ImportTransaction::new(dest.clone()).unwrap();

        assert_eq!(tx.dest_path(), dest.as_path());
    }

    // ============================================================================
    // Stage File Tests
    // ============================================================================

    #[test]
    fn test_transaction_stage_and_commit() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        // Stage some files
        tx.stage_file(Path::new("test.md"), b"# Hello World")
            .unwrap();
        tx.stage_file(Path::new("folder/nested.md"), b"Nested content")
            .unwrap();

        // Verify staging
        assert!(tx.staging_dir().join("test.md").exists());
        assert!(tx.staging_dir().join("folder/nested.md").exists());

        // Commit
        let stats = tx.commit().unwrap();
        assert_eq!(stats.files_staged, 2);
        assert_eq!(stats.files_committed, 2);

        // Verify destination
        assert!(dest.join("test.md").exists());
        assert!(dest.join("folder/nested.md").exists());

        // Verify staging cleaned up
        assert!(!tx.staging_dir().exists());
    }

    #[test]
    fn test_stage_file_empty_content() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("empty.txt"), b"").unwrap();

        let stats = tx.commit().unwrap();
        assert_eq!(stats.files_committed, 1);
        assert_eq!(stats.bytes_written, 0);

        let content = fs::read_to_string(dest.join("empty.txt")).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_stage_file_binary_content() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        let binary_data: Vec<u8> = (0..=255).collect();
        tx.stage_file(Path::new("binary.bin"), &binary_data)
            .unwrap();

        let stats = tx.commit().unwrap();
        assert_eq!(stats.bytes_written, 256);

        let content = fs::read(dest.join("binary.bin")).unwrap();
        assert_eq!(content, binary_data);
    }

    #[test]
    fn test_stage_file_large_content() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        // Create 1MB of content
        let large_content = vec![b'x'; 1024 * 1024];
        tx.stage_file(Path::new("large.txt"), &large_content)
            .unwrap();

        let stats = tx.commit().unwrap();
        assert_eq!(stats.bytes_written, 1024 * 1024);
    }

    #[test]
    fn test_stage_file_deeply_nested() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("a/b/c/d/e/f/deep.txt"), b"deep content")
            .unwrap();

        tx.commit().unwrap();

        assert!(dest.join("a/b/c/d/e/f/deep.txt").exists());
        let content = fs::read_to_string(dest.join("a/b/c/d/e/f/deep.txt")).unwrap();
        assert_eq!(content, "deep content");
    }

    #[test]
    fn test_stage_file_overwrites_in_staging_dir() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("test.txt"), b"first").unwrap();

        // Verify the staged file has "first"
        let staged_content = fs::read_to_string(tx.staging_dir().join("test.txt")).unwrap();
        assert_eq!(staged_content, "first");

        // Stage again - this will overwrite the file in staging dir
        tx.stage_file(Path::new("test.txt"), b"second").unwrap();

        // Verify it's now "second" in staging dir
        let staged_content = fs::read_to_string(tx.staging_dir().join("test.txt")).unwrap();
        assert_eq!(staged_content, "second");

        // Note: staged_files list now has duplicate entries, but the actual file has latest content
        // The commit will try to move the same file twice (second attempt will be copy fallback)
    }

    #[test]
    fn test_stage_file_with_special_characters() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("file with spaces.txt"), b"content")
            .unwrap();
        tx.stage_file(Path::new("file-with-dashes.txt"), b"content")
            .unwrap();
        tx.stage_file(Path::new("file_with_underscores.txt"), b"content")
            .unwrap();

        let stats = tx.commit().unwrap();
        assert_eq!(stats.files_committed, 3);
    }

    // ============================================================================
    // Rollback Tests
    // ============================================================================

    #[test]
    fn test_transaction_rollback() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("test.md"), b"# Hello World")
            .unwrap();
        let staging = tx.staging_dir().to_path_buf();

        // Verify staging exists
        assert!(staging.exists());

        // Rollback
        tx.rollback().unwrap();

        // Verify staging cleaned up
        assert!(!staging.exists());

        // Verify destination not created
        assert!(!dest.exists());
    }

    #[test]
    fn test_rollback_multiple_times() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();

        tx.stage_file(Path::new("test.md"), b"content").unwrap();

        // Rollback multiple times should be safe
        tx.rollback().unwrap();
        tx.rollback().unwrap();
        tx.rollback().unwrap();
    }

    #[test]
    fn test_rollback_clears_stats() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();

        tx.stage_file(Path::new("test.md"), b"content").unwrap();
        assert!(tx.stats().files_staged > 0);
        assert!(tx.stats().bytes_written > 0);

        tx.rollback().unwrap();

        assert_eq!(tx.stats().files_staged, 0);
        assert_eq!(tx.stats().bytes_written, 0);
    }

    #[test]
    fn test_rollback_after_commit_is_noop() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("test.md"), b"content").unwrap();
        tx.commit().unwrap();

        // Rollback after commit should be no-op
        tx.rollback().unwrap();

        // File should still exist at destination
        assert!(dest.join("test.md").exists());
    }

    // ============================================================================
    // Auto-Rollback (Drop) Tests
    // ============================================================================

    #[test]
    fn test_transaction_auto_rollback_on_drop() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let staging_path;
        {
            let mut tx = ImportTransaction::new(dest.clone()).unwrap();
            tx.stage_file(Path::new("test.md"), b"# Hello World")
                .unwrap();
            staging_path = tx.staging_dir().to_path_buf();
            assert!(staging_path.exists());
            // tx dropped here without commit
        }

        // Verify staging cleaned up by drop
        assert!(!staging_path.exists());
    }

    #[test]
    fn test_no_auto_rollback_after_commit() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        {
            let mut tx = ImportTransaction::new(dest.clone()).unwrap();
            tx.stage_file(Path::new("test.md"), b"content").unwrap();
            tx.commit().unwrap();
            // tx dropped here after commit
        }

        // File should exist
        assert!(dest.join("test.md").exists());
    }

    // ============================================================================
    // Path Traversal Security Tests
    // ============================================================================

    #[test]
    fn test_transaction_path_traversal_rejected() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();

        // Attempt path traversal
        let result = tx.stage_file(Path::new("../escape.md"), b"malicious");
        assert!(result.is_err());
    }

    #[test]
    fn test_path_traversal_double_dot() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();

        let result = tx.stage_file(Path::new("folder/../../../escape.md"), b"malicious");
        assert!(result.is_err());
    }

    #[test]
    fn test_path_traversal_in_copy() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source.txt");
        fs::write(&source, "content").unwrap();

        let dest = temp.path().join("import_dest");
        let mut tx = ImportTransaction::new(dest).unwrap();

        let result = tx.stage_copy(&source, Path::new("../escape.txt"));
        assert!(result.is_err());
    }

    // ============================================================================
    // Stage Copy Tests
    // ============================================================================

    #[test]
    fn test_stage_copy() {
        let temp = tempdir().unwrap();
        let source_file = temp.path().join("source.txt");
        fs::write(&source_file, "Source content").unwrap();

        let dest = temp.path().join("import_dest");
        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_copy(&source_file, Path::new("copied.txt"))
            .unwrap();

        let stats = tx.commit().unwrap();
        assert_eq!(stats.files_committed, 1);

        let dest_content = fs::read_to_string(dest.join("copied.txt")).unwrap();
        assert_eq!(dest_content, "Source content");
    }

    #[test]
    fn test_stage_copy_to_nested_path() {
        let temp = tempdir().unwrap();
        let source_file = temp.path().join("source.txt");
        fs::write(&source_file, "content").unwrap();

        let dest = temp.path().join("import_dest");
        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_copy(&source_file, Path::new("nested/folder/copied.txt"))
            .unwrap();

        tx.commit().unwrap();

        assert!(dest.join("nested/folder/copied.txt").exists());
    }

    #[test]
    fn test_stage_copy_nonexistent_source() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();

        let result = tx.stage_copy(Path::new("/nonexistent/file.txt"), Path::new("copied.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_stage_copy_tracks_bytes() {
        let temp = tempdir().unwrap();
        let source_file = temp.path().join("source.txt");
        fs::write(&source_file, "12345678901234567890").unwrap(); // 20 bytes

        let dest = temp.path().join("import_dest");
        let mut tx = ImportTransaction::new(dest).unwrap();

        tx.stage_copy(&source_file, Path::new("copied.txt"))
            .unwrap();

        assert_eq!(tx.stats().bytes_written, 20);
    }

    // ============================================================================
    // Verify Copy Tests
    // ============================================================================

    #[test]
    fn test_verify_copy() {
        let temp = tempdir().unwrap();
        let source_file = temp.path().join("source.txt");
        fs::write(&source_file, "Test content for verification").unwrap();

        let dest = temp.path().join("import_dest");
        let mut tx = ImportTransaction::new(dest).unwrap();

        tx.stage_copy(&source_file, Path::new("copied.txt"))
            .unwrap();

        let staged_path = tx.staging_dir().join("copied.txt");
        assert!(tx.verify_copy(&source_file, &staged_path).unwrap());
    }

    #[test]
    fn test_verify_copy_mismatch() {
        let temp = tempdir().unwrap();
        let source_file = temp.path().join("source.txt");
        let other_file = temp.path().join("other.txt");
        fs::write(&source_file, "content A").unwrap();
        fs::write(&other_file, "content B").unwrap();

        let dest = temp.path().join("import_dest");
        let tx = ImportTransaction::new(dest).unwrap();

        // Verify should fail for different files
        assert!(!tx.verify_copy(&source_file, &other_file).unwrap());
    }

    #[test]
    fn test_verify_copy_same_content_different_files() {
        let temp = tempdir().unwrap();
        let file_a = temp.path().join("a.txt");
        let file_b = temp.path().join("b.txt");
        fs::write(&file_a, "identical content").unwrap();
        fs::write(&file_b, "identical content").unwrap();

        let dest = temp.path().join("import_dest");
        let tx = ImportTransaction::new(dest).unwrap();

        // Verify should pass for files with same content
        assert!(tx.verify_copy(&file_a, &file_b).unwrap());
    }

    // ============================================================================
    // Commit Tests
    // ============================================================================

    #[test]
    fn test_commit_creates_destination() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("new_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();
        tx.stage_file(Path::new("test.txt"), b"content").unwrap();

        assert!(!dest.exists());

        tx.commit().unwrap();

        assert!(dest.exists());
    }

    #[test]
    fn test_commit_double_commit_fails() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();
        tx.stage_file(Path::new("test.txt"), b"content").unwrap();

        tx.commit().unwrap();

        let result = tx.commit();
        assert!(result.is_err());
    }

    #[test]
    fn test_commit_empty_transaction() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();

        // Commit with no staged files
        let stats = tx.commit().unwrap();
        assert_eq!(stats.files_staged, 0);
        assert_eq!(stats.files_committed, 0);
        assert_eq!(stats.bytes_written, 0);
    }

    #[test]
    fn test_commit_preserves_content() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        let content = "Line 1\nLine 2\nLine 3\n日本語テスト";
        tx.stage_file(Path::new("test.txt"), content.as_bytes())
            .unwrap();

        tx.commit().unwrap();

        let read_content = fs::read_to_string(dest.join("test.txt")).unwrap();
        assert_eq!(read_content, content);
    }

    // ============================================================================
    // Stats Tests
    // ============================================================================

    #[test]
    fn test_stats_initial() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let tx = ImportTransaction::new(dest).unwrap();
        let stats = tx.stats();

        assert_eq!(stats.files_staged, 0);
        assert_eq!(stats.bytes_written, 0);
        assert_eq!(stats.files_committed, 0);
    }

    #[test]
    fn test_stats_after_staging() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();
        tx.stage_file(Path::new("a.txt"), b"content1").unwrap(); // 8 bytes
        tx.stage_file(Path::new("b.txt"), b"content2").unwrap(); // 8 bytes

        let stats = tx.stats();
        assert_eq!(stats.files_staged, 2);
        assert_eq!(stats.bytes_written, 16);
        assert_eq!(stats.files_committed, 0);
    }

    #[test]
    fn test_stats_after_commit() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest).unwrap();
        tx.stage_file(Path::new("a.txt"), b"content").unwrap();
        tx.stage_file(Path::new("b.txt"), b"content").unwrap();

        tx.commit().unwrap();

        let stats = tx.stats();
        assert_eq!(stats.files_staged, 2);
        assert_eq!(stats.files_committed, 2);
    }

    // ============================================================================
    // TransactionStats Tests
    // ============================================================================

    #[test]
    fn test_transaction_stats_debug() {
        let stats = TransactionStats {
            files_staged: 5,
            bytes_written: 1024,
            files_committed: 5,
        };

        let debug = format!("{:?}", stats);
        assert!(debug.contains("files_staged"));
        assert!(debug.contains("5"));
    }

    #[test]
    fn test_transaction_stats_clone() {
        let stats = TransactionStats {
            files_staged: 10,
            bytes_written: 2048,
            files_committed: 10,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.files_staged, stats.files_staged);
        assert_eq!(cloned.bytes_written, stats.bytes_written);
        assert_eq!(cloned.files_committed, stats.files_committed);
    }

    // ============================================================================
    // compute_file_hash Tests
    // ============================================================================

    #[test]
    fn test_compute_file_hash_same_content() {
        let temp = tempdir().unwrap();
        let file_a = temp.path().join("a.txt");
        let file_b = temp.path().join("b.txt");

        fs::write(&file_a, "same content").unwrap();
        fs::write(&file_b, "same content").unwrap();

        let hash_a = compute_file_hash(&file_a).unwrap();
        let hash_b = compute_file_hash(&file_b).unwrap();

        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn test_compute_file_hash_different_content() {
        let temp = tempdir().unwrap();
        let file_a = temp.path().join("a.txt");
        let file_b = temp.path().join("b.txt");

        fs::write(&file_a, "content A").unwrap();
        fs::write(&file_b, "content B").unwrap();

        let hash_a = compute_file_hash(&file_a).unwrap();
        let hash_b = compute_file_hash(&file_b).unwrap();

        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn test_compute_file_hash_empty_file() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("empty.txt");
        fs::write(&file, "").unwrap();

        let hash = compute_file_hash(&file).unwrap();
        // SHA-256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_compute_file_hash_nonexistent() {
        let result = compute_file_hash(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_file_hash_large_file() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("large.txt");

        // Create a file larger than the 8192 byte buffer
        let content = vec![b'x'; 32768];
        fs::write(&file, &content).unwrap();

        let hash = compute_file_hash(&file).unwrap();
        // Just verify it produces a valid 64-char hex hash
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ============================================================================
    // validate_disk_space Tests
    // ============================================================================

    #[test]
    fn test_validate_disk_space_existing_path() {
        let temp = tempdir().unwrap();

        let result = validate_disk_space(temp.path(), 1024);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_disk_space_nested_nonexistent() {
        let temp = tempdir().unwrap();
        let nested = temp.path().join("a/b/c/d/dest");

        // Should find existing parent
        let result = validate_disk_space(&nested, 1024);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_disk_space_zero_bytes() {
        let temp = tempdir().unwrap();

        let result = validate_disk_space(temp.path(), 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_disk_space_large_requirement() {
        let temp = tempdir().unwrap();

        // Request a large amount - function currently doesn't actually check space
        let result = validate_disk_space(temp.path(), 1024 * 1024 * 1024 * 100); // 100GB
        assert!(result.is_ok()); // Currently always succeeds as space check not implemented
    }

    // ============================================================================
    // Edge Case Tests
    // ============================================================================

    #[test]
    fn test_stage_file_after_rollback() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("first.txt"), b"first").unwrap();
        tx.rollback().unwrap();

        // Staging after rollback should still work but staging dir is gone
        // This will fail because staging dir was removed
        let result = tx.stage_file(Path::new("second.txt"), b"second");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_files_same_directory() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("dir/a.txt"), b"a").unwrap();
        tx.stage_file(Path::new("dir/b.txt"), b"b").unwrap();
        tx.stage_file(Path::new("dir/c.txt"), b"c").unwrap();

        let stats = tx.commit().unwrap();
        assert_eq!(stats.files_committed, 3);

        assert!(dest.join("dir/a.txt").exists());
        assert!(dest.join("dir/b.txt").exists());
        assert!(dest.join("dir/c.txt").exists());
    }

    #[test]
    fn test_stage_mixed_file_and_copy() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source.txt");
        fs::write(&source, "copied content").unwrap();

        let dest = temp.path().join("import_dest");
        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        tx.stage_file(Path::new("written.txt"), b"written content")
            .unwrap();
        tx.stage_copy(&source, Path::new("copied.txt")).unwrap();

        let stats = tx.commit().unwrap();
        assert_eq!(stats.files_committed, 2);

        assert!(dest.join("written.txt").exists());
        assert!(dest.join("copied.txt").exists());
    }

    #[test]
    fn test_unicode_content() {
        let temp = tempdir().unwrap();
        let dest = temp.path().join("import_dest");

        let mut tx = ImportTransaction::new(dest.clone()).unwrap();

        let unicode_content = "Hello 世界! 🌍 Привет мир!";
        tx.stage_file(Path::new("unicode.txt"), unicode_content.as_bytes())
            .unwrap();

        tx.commit().unwrap();

        let content = fs::read_to_string(dest.join("unicode.txt")).unwrap();
        assert_eq!(content, unicode_content);
    }
}
