//! File system abstraction for testability.
//!
//! Provides a trait for file system operations that can be mocked in tests.

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Result type for file system operations.
pub type FsResult<T> = Result<T, std::io::Error>;

/// Abstraction over file system operations for testability.
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// Read the entire contents of a file as a string.
    async fn read_to_string(&self, path: &Path) -> FsResult<String>;

    /// Read the entire contents of a file as bytes.
    async fn read(&self, path: &Path) -> FsResult<Vec<u8>>;

    /// Write string content to a file, creating it if it doesn't exist.
    async fn write(&self, path: &Path, content: &str) -> FsResult<()>;

    /// Write bytes to a file, creating it if it doesn't exist.
    async fn write_bytes(&self, path: &Path, content: &[u8]) -> FsResult<()>;

    /// Create a directory and all parent directories.
    async fn create_dir_all(&self, path: &Path) -> FsResult<()>;

    /// Remove a file.
    async fn remove_file(&self, path: &Path) -> FsResult<()>;

    /// Remove a directory and all its contents.
    async fn remove_dir_all(&self, path: &Path) -> FsResult<()>;

    /// Check if a path exists.
    async fn exists(&self, path: &Path) -> bool;

    /// Check if a path is a file.
    async fn is_file(&self, path: &Path) -> bool;

    /// Check if a path is a directory.
    async fn is_dir(&self, path: &Path) -> bool;

    /// List entries in a directory.
    async fn read_dir(&self, path: &Path) -> FsResult<Vec<PathBuf>>;

    /// Rename/move a file or directory.
    async fn rename(&self, from: &Path, to: &Path) -> FsResult<()>;

    /// Copy a file.
    async fn copy(&self, from: &Path, to: &Path) -> FsResult<u64>;

    /// Get file metadata (size, modified time, etc.)
    async fn metadata(&self, path: &Path) -> FsResult<std::fs::Metadata>;

    /// Canonicalize a path (resolve symlinks and relative paths).
    fn canonicalize(&self, path: &Path) -> FsResult<PathBuf>;
}

/// Real implementation using Tokio's async file system.
#[derive(Debug, Clone, Default)]
pub struct TokioFileSystem;

impl TokioFileSystem {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl FileSystem for TokioFileSystem {
    async fn read_to_string(&self, path: &Path) -> FsResult<String> {
        fs::read_to_string(path).await
    }

    async fn read(&self, path: &Path) -> FsResult<Vec<u8>> {
        fs::read(path).await
    }

    async fn write(&self, path: &Path, content: &str) -> FsResult<()> {
        fs::write(path, content).await
    }

    async fn write_bytes(&self, path: &Path, content: &[u8]) -> FsResult<()> {
        fs::write(path, content).await
    }

    async fn create_dir_all(&self, path: &Path) -> FsResult<()> {
        fs::create_dir_all(path).await
    }

    async fn remove_file(&self, path: &Path) -> FsResult<()> {
        fs::remove_file(path).await
    }

    async fn remove_dir_all(&self, path: &Path) -> FsResult<()> {
        fs::remove_dir_all(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        fs::try_exists(path).await.unwrap_or(false)
    }

    async fn is_file(&self, path: &Path) -> bool {
        fs::metadata(path)
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
    }

    async fn is_dir(&self, path: &Path) -> bool {
        fs::metadata(path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
    }

    async fn read_dir(&self, path: &Path) -> FsResult<Vec<PathBuf>> {
        let mut entries = Vec::new();
        let mut dir = fs::read_dir(path).await?;
        while let Some(entry) = dir.next_entry().await? {
            entries.push(entry.path());
        }
        Ok(entries)
    }

    async fn rename(&self, from: &Path, to: &Path) -> FsResult<()> {
        fs::rename(from, to).await
    }

    async fn copy(&self, from: &Path, to: &Path) -> FsResult<u64> {
        fs::copy(from, to).await
    }

    async fn metadata(&self, path: &Path) -> FsResult<std::fs::Metadata> {
        fs::metadata(path).await
    }

    fn canonicalize(&self, path: &Path) -> FsResult<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// Mock implementation for testing.
#[cfg(test)]
pub use mock::MockFileSystem;

#[cfg(test)]
mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    /// In-memory mock file system for testing.
    #[derive(Debug, Clone, Default)]
    pub struct MockFileSystem {
        files: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
        dirs: Arc<RwLock<std::collections::HashSet<PathBuf>>>,
    }

    impl MockFileSystem {
        pub fn new() -> Self {
            Self {
                files: Arc::new(RwLock::new(HashMap::new())),
                dirs: Arc::new(RwLock::new(std::collections::HashSet::new())),
            }
        }

        /// Pre-populate a file for testing.
        pub fn with_file(self, path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Self {
            self.files
                .write()
                .unwrap()
                .insert(path.as_ref().to_path_buf(), content.as_ref().to_vec());
            self
        }

        /// Pre-populate a directory for testing.
        pub fn with_dir(self, path: impl AsRef<Path>) -> Self {
            self.dirs
                .write()
                .unwrap()
                .insert(path.as_ref().to_path_buf());
            self
        }

        /// Get the current contents of a file (for assertions).
        pub fn get_file(&self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
            self.files.read().unwrap().get(path.as_ref()).cloned()
        }

        /// Check if a file exists (for assertions).
        pub fn has_file(&self, path: impl AsRef<Path>) -> bool {
            self.files.read().unwrap().contains_key(path.as_ref())
        }
    }

    #[async_trait]
    impl FileSystem for MockFileSystem {
        async fn read_to_string(&self, path: &Path) -> FsResult<String> {
            self.files
                .read()
                .unwrap()
                .get(path)
                .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
        }

        async fn read(&self, path: &Path) -> FsResult<Vec<u8>> {
            self.files
                .read()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
        }

        async fn write(&self, path: &Path, content: &str) -> FsResult<()> {
            self.files
                .write()
                .unwrap()
                .insert(path.to_path_buf(), content.as_bytes().to_vec());
            Ok(())
        }

        async fn write_bytes(&self, path: &Path, content: &[u8]) -> FsResult<()> {
            self.files
                .write()
                .unwrap()
                .insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        async fn create_dir_all(&self, path: &Path) -> FsResult<()> {
            self.dirs.write().unwrap().insert(path.to_path_buf());
            Ok(())
        }

        async fn remove_file(&self, path: &Path) -> FsResult<()> {
            self.files
                .write()
                .unwrap()
                .remove(path)
                .map(|_| ())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
        }

        async fn remove_dir_all(&self, path: &Path) -> FsResult<()> {
            self.dirs.write().unwrap().remove(path);
            // Also remove all files under this directory
            self.files
                .write()
                .unwrap()
                .retain(|k, _| !k.starts_with(path));
            Ok(())
        }

        async fn exists(&self, path: &Path) -> bool {
            self.files.read().unwrap().contains_key(path)
                || self.dirs.read().unwrap().contains(path)
        }

        async fn is_file(&self, path: &Path) -> bool {
            self.files.read().unwrap().contains_key(path)
        }

        async fn is_dir(&self, path: &Path) -> bool {
            self.dirs.read().unwrap().contains(path)
        }

        async fn read_dir(&self, path: &Path) -> FsResult<Vec<PathBuf>> {
            let files = self.files.read().unwrap();
            let dirs = self.dirs.read().unwrap();

            let mut entries: Vec<PathBuf> = files
                .keys()
                .filter(|p| p.parent().map(|parent| parent == path).unwrap_or(false))
                .cloned()
                .collect();

            entries.extend(
                dirs.iter()
                    .filter(|p| p.parent().map(|parent| parent == path).unwrap_or(false))
                    .cloned(),
            );

            Ok(entries)
        }

        async fn rename(&self, from: &Path, to: &Path) -> FsResult<()> {
            if let Some(content) = self.files.write().unwrap().remove(from) {
                self.files
                    .write()
                    .unwrap()
                    .insert(to.to_path_buf(), content);
                Ok(())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "File not found",
                ))
            }
        }

        async fn copy(&self, from: &Path, to: &Path) -> FsResult<u64> {
            let content = self
                .files
                .read()
                .unwrap()
                .get(from)
                .cloned()
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "File not found")
                })?;
            let len = content.len() as u64;
            self.files
                .write()
                .unwrap()
                .insert(to.to_path_buf(), content);
            Ok(len)
        }

        async fn metadata(&self, _path: &Path) -> FsResult<std::fs::Metadata> {
            // Mock metadata is tricky - we'll return an error for now
            // In real tests, you might want to extend MockFileSystem to track metadata
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Metadata not supported in mock",
            ))
        }

        fn canonicalize(&self, path: &Path) -> FsResult<PathBuf> {
            // Just return the path as-is for mock
            Ok(path.to_path_buf())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_tokio_fs_write_and_read() {
        let dir = tempdir().unwrap();
        let fs = TokioFileSystem::new();
        let file_path = dir.path().join("test.txt");

        fs.write(&file_path, "Hello, World!").await.unwrap();
        let content = fs.read_to_string(&file_path).await.unwrap();

        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_mock_fs_write_and_read() {
        let fs = MockFileSystem::new();
        let path = PathBuf::from("/test/file.txt");

        fs.write(&path, "Test content").await.unwrap();
        let content = fs.read_to_string(&path).await.unwrap();

        assert_eq!(content, "Test content");
    }

    #[tokio::test]
    async fn test_mock_fs_with_file() {
        let fs = MockFileSystem::new().with_file("/test.txt", "Pre-existing content");

        let content = fs.read_to_string(Path::new("/test.txt")).await.unwrap();
        assert_eq!(content, "Pre-existing content");
    }

    #[tokio::test]
    async fn test_mock_fs_exists() {
        let fs = MockFileSystem::new()
            .with_file("/file.txt", "content")
            .with_dir("/dir");

        assert!(fs.exists(Path::new("/file.txt")).await);
        assert!(fs.exists(Path::new("/dir")).await);
        assert!(!fs.exists(Path::new("/nonexistent")).await);
    }
}
