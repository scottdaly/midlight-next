//! Object store abstraction for testability.
//!
//! Provides a trait for content-addressable storage operations.

use async_trait::async_trait;

/// Error type for object store operations.
#[derive(Debug, thiserror::Error)]
pub enum ObjectStoreError {
    #[error("Object not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Result type for object store operations.
pub type ObjectStoreResult<T> = Result<T, ObjectStoreError>;

/// Abstraction over content-addressable storage for testability.
///
/// Objects are stored by their SHA-256 hash, enabling deduplication.
#[async_trait]
pub trait ObjectStoreOps: Send + Sync {
    /// Write content to the store and return its hash.
    ///
    /// If the content already exists, returns the existing hash without writing.
    async fn write(&self, content: &str) -> ObjectStoreResult<String>;

    /// Read content by its hash.
    async fn read(&self, hash: &str) -> ObjectStoreResult<String>;

    /// Check if an object exists.
    async fn exists(&self, hash: &str) -> bool;

    /// Delete an object by its hash.
    async fn delete(&self, hash: &str) -> ObjectStoreResult<()>;

    /// Initialize the store (create directories, etc.)
    async fn init(&self) -> ObjectStoreResult<()>;
}

/// Mock implementation for testing.
#[cfg(test)]
pub use mock::MockObjectStore;

#[cfg(test)]
mod mock {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    /// In-memory mock object store for testing.
    #[derive(Debug, Clone, Default)]
    pub struct MockObjectStore {
        objects: Arc<RwLock<HashMap<String, String>>>,
        write_count: Arc<RwLock<usize>>,
    }

    impl MockObjectStore {
        pub fn new() -> Self {
            Self {
                objects: Arc::new(RwLock::new(HashMap::new())),
                write_count: Arc::new(RwLock::new(0)),
            }
        }

        /// Pre-populate an object for testing.
        pub fn with_object(self, hash: impl Into<String>, content: impl Into<String>) -> Self {
            self.objects
                .write()
                .unwrap()
                .insert(hash.into(), content.into());
            self
        }

        /// Get the number of writes performed (useful for testing deduplication).
        pub fn write_count(&self) -> usize {
            *self.write_count.read().unwrap()
        }

        /// Get all stored hashes.
        pub fn hashes(&self) -> Vec<String> {
            self.objects.read().unwrap().keys().cloned().collect()
        }

        fn compute_hash(content: &str) -> String {
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            format!("{:x}", hasher.finalize())
        }
    }

    #[async_trait]
    impl ObjectStoreOps for MockObjectStore {
        async fn write(&self, content: &str) -> ObjectStoreResult<String> {
            let hash = Self::compute_hash(content);

            let mut objects = self.objects.write().unwrap();
            if !objects.contains_key(&hash) {
                objects.insert(hash.clone(), content.to_string());
                *self.write_count.write().unwrap() += 1;
            }

            Ok(hash)
        }

        async fn read(&self, hash: &str) -> ObjectStoreResult<String> {
            self.objects
                .read()
                .unwrap()
                .get(hash)
                .cloned()
                .ok_or_else(|| ObjectStoreError::NotFound(hash.to_string()))
        }

        async fn exists(&self, hash: &str) -> bool {
            self.objects.read().unwrap().contains_key(hash)
        }

        async fn delete(&self, hash: &str) -> ObjectStoreResult<()> {
            self.objects
                .write()
                .unwrap()
                .remove(hash)
                .map(|_| ())
                .ok_or_else(|| ObjectStoreError::NotFound(hash.to_string()))
        }

        async fn init(&self) -> ObjectStoreResult<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_object_store_write_and_read() {
        let store = MockObjectStore::new();

        let hash = store.write("Hello, World!").await.unwrap();
        let content = store.read(&hash).await.unwrap();

        assert_eq!(content, "Hello, World!");
        assert!(store.exists(&hash).await);
    }

    #[tokio::test]
    async fn test_mock_object_store_deduplication() {
        let store = MockObjectStore::new();

        let hash1 = store.write("Same content").await.unwrap();
        let hash2 = store.write("Same content").await.unwrap();

        assert_eq!(hash1, hash2);
        assert_eq!(store.write_count(), 1); // Only one actual write
    }

    #[tokio::test]
    async fn test_mock_object_store_not_found() {
        let store = MockObjectStore::new();

        let result = store.read("nonexistent").await;
        assert!(matches!(result, Err(ObjectStoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_mock_object_store_delete() {
        let store = MockObjectStore::new();

        let hash = store.write("To be deleted").await.unwrap();
        assert!(store.exists(&hash).await);

        store.delete(&hash).await.unwrap();
        assert!(!store.exists(&hash).await);
    }

    #[tokio::test]
    async fn test_mock_object_store_with_object() {
        let store = MockObjectStore::new().with_object("abc123", "Pre-existing content");

        let content = store.read("abc123").await.unwrap();
        assert_eq!(content, "Pre-existing content");
    }
}
