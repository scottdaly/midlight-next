// Content-addressable object store using SHA-256 hashes
// Similar to Git's object storage model

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::error::{MidlightError, Result};
use crate::traits::{
    object_store::ObjectStoreError, object_store::ObjectStoreResult, ObjectStoreOps,
};
use async_trait::async_trait;

/// Content-addressable storage using SHA-256 hashes
/// Objects are stored compressed (gzip) in a git-like directory structure:
/// .midlight/objects/XX/XXXXXX... (first 2 chars as subdirectory)
pub struct ObjectStore {
    objects_dir: PathBuf,
}

impl ObjectStore {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            objects_dir: workspace_root.join(".midlight").join("objects"),
        }
    }

    /// Initialize the object store directory
    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.objects_dir)?;
        Ok(())
    }

    /// Calculate SHA-256 hash of content
    pub fn hash(&self, content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Store content and return its hash
    /// If content already exists (same hash), returns hash without re-storing
    pub async fn write(&self, content: &str) -> Result<String> {
        let hash = self.hash(content);
        let object_path = self.get_object_path(&hash);

        // Deduplication: if already exists, skip
        if object_path.exists() {
            return Ok(hash);
        }

        // Ensure parent directory exists
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Compress and write
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content.as_bytes())?;
        let compressed = encoder.finish()?;

        fs::write(&object_path, compressed)?;

        tracing::debug!("Stored object: {} ({} bytes)", &hash[..8], content.len());

        Ok(hash)
    }

    /// Read content by hash
    pub async fn read(&self, hash: &str) -> Result<String> {
        let object_path = self.get_object_path(hash);

        if !object_path.exists() {
            return Err(MidlightError::ObjectNotFound(hash.to_string()));
        }

        let compressed = fs::read(&object_path)?;

        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut content = String::new();
        decoder.read_to_string(&mut content)?;

        Ok(content)
    }

    /// Check if object exists
    #[allow(dead_code)]
    pub async fn exists(&self, hash: &str) -> bool {
        self.get_object_path(hash).exists()
    }

    /// Get the file path for an object hash
    /// Uses git-like structure: first 2 chars as subdirectory
    fn get_object_path(&self, hash: &str) -> PathBuf {
        if hash.len() < 2 {
            return self.objects_dir.join(hash);
        }
        self.objects_dir.join(&hash[..2]).join(&hash[2..])
    }

    /// Get total size of all objects in bytes
    #[allow(dead_code)]
    pub async fn total_size(&self) -> Result<u64> {
        let mut total = 0u64;

        if !self.objects_dir.exists() {
            return Ok(0);
        }

        for entry in fs::read_dir(&self.objects_dir)?.flatten() {
            if entry.path().is_dir() {
                for file in fs::read_dir(entry.path())?.flatten() {
                    if let Ok(metadata) = file.metadata() {
                        total += metadata.len();
                    }
                }
            }
        }

        Ok(total)
    }

    /// Garbage collect unreferenced objects
    /// Takes a set of hashes that are still in use
    #[allow(dead_code)]
    pub async fn gc(&self, used_hashes: &std::collections::HashSet<String>) -> Result<u32> {
        let mut deleted = 0u32;

        if !self.objects_dir.exists() {
            return Ok(0);
        }

        for entry in fs::read_dir(&self.objects_dir)?.flatten() {
            if entry.path().is_dir() {
                for file in fs::read_dir(entry.path())?.flatten() {
                    let file_name = file.file_name().to_string_lossy().to_string();
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    let hash = format!("{}{}", dir_name, file_name);

                    if !used_hashes.contains(&hash) {
                        fs::remove_file(file.path())?;
                        deleted += 1;
                    }
                }
            }
        }

        tracing::info!("GC: deleted {} unreferenced objects", deleted);

        Ok(deleted)
    }
}

/// Implement the ObjectStoreOps trait for ObjectStore
/// This allows ObjectStore to be used with generic services that depend on the trait
#[async_trait]
impl ObjectStoreOps for ObjectStore {
    async fn write(&self, content: &str) -> ObjectStoreResult<String> {
        ObjectStore::write(self, content)
            .await
            .map_err(|e| ObjectStoreError::StorageError(e.to_string()))
    }

    async fn read(&self, hash: &str) -> ObjectStoreResult<String> {
        ObjectStore::read(self, hash).await.map_err(|e| match e {
            MidlightError::ObjectNotFound(h) => ObjectStoreError::NotFound(h),
            other => ObjectStoreError::StorageError(other.to_string()),
        })
    }

    async fn exists(&self, hash: &str) -> bool {
        ObjectStore::exists(self, hash).await
    }

    async fn delete(&self, hash: &str) -> ObjectStoreResult<()> {
        let object_path = self.get_object_path(hash);
        if object_path.exists() {
            fs::remove_file(object_path).map_err(|e| ObjectStoreError::IoError(e))
        } else {
            Err(ObjectStoreError::NotFound(hash.to_string()))
        }
    }

    async fn init(&self) -> ObjectStoreResult<()> {
        ObjectStore::init(self)
            .await
            .map_err(|e| ObjectStoreError::StorageError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::tempdir;

    // ============================================
    // Basic read/write operations
    // ============================================

    #[tokio::test]
    async fn test_write_and_read() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let content = "Hello, World!";
        let hash = store.write(content).await.unwrap();

        assert_eq!(store.hash(content), hash);

        let read_content = store.read(&hash).await.unwrap();
        assert_eq!(read_content, content);
    }

    #[tokio::test]
    async fn test_deduplication() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let content = "Duplicate content";
        let hash1 = store.write(content).await.unwrap();
        let hash2 = store.write(content).await.unwrap();

        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_not_found() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let result = store.read("nonexistent").await;
        assert!(result.is_err());
    }

    // ============================================
    // Hash computation
    // ============================================

    #[tokio::test]
    async fn test_hash_determinism() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());

        let content = "Test content for hashing";

        // Same content should always produce same hash
        let hash1 = store.hash(content);
        let hash2 = store.hash(content);
        assert_eq!(hash1, hash2);

        // SHA-256 produces 64 hex characters
        assert_eq!(hash1.len(), 64);
    }

    #[tokio::test]
    async fn test_hash_uniqueness() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());

        // Different content should produce different hashes
        let hash1 = store.hash("content A");
        let hash2 = store.hash("content B");
        let hash3 = store.hash("content C");

        assert_ne!(hash1, hash2);
        assert_ne!(hash2, hash3);
        assert_ne!(hash1, hash3);
    }

    #[tokio::test]
    async fn test_hash_empty_content() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());

        // Empty content should have a valid hash
        let hash = store.hash("");
        assert_eq!(hash.len(), 64);

        // SHA-256 of empty string is well-known
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn test_hash_unicode_content() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());

        // Unicode content should hash correctly
        let hash1 = store.hash("Hello 世界");
        let hash2 = store.hash("Hello 世界");
        assert_eq!(hash1, hash2);

        // Different unicode should produce different hash
        let hash3 = store.hash("Hello 世界!");
        assert_ne!(hash1, hash3);
    }

    // ============================================
    // Exists operation
    // ============================================

    #[tokio::test]
    async fn test_exists() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let content = "Test content";
        let hash = store.write(content).await.unwrap();

        assert!(store.exists(&hash).await);
        assert!(!store.exists("nonexistent_hash").await);
    }

    // ============================================
    // Large content handling
    // ============================================

    #[tokio::test]
    async fn test_large_content() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        // Create 1MB of content
        let large_content = "x".repeat(1024 * 1024);

        let hash = store.write(&large_content).await.unwrap();
        let read_content = store.read(&hash).await.unwrap();

        assert_eq!(read_content, large_content);
    }

    #[tokio::test]
    async fn test_compression_efficiency() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        // Highly compressible content
        let content = "a".repeat(10000);

        let hash = store.write(&content).await.unwrap();

        // Read the compressed file directly
        let object_path = temp
            .path()
            .join(".midlight")
            .join("objects")
            .join(&hash[..2])
            .join(&hash[2..]);

        let compressed_size = std::fs::metadata(&object_path).unwrap().len();

        // Compressed should be much smaller than original
        assert!(compressed_size < content.len() as u64 / 10);
    }

    // ============================================
    // Unicode and special content
    // ============================================

    #[tokio::test]
    async fn test_unicode_content_roundtrip() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let unicode_content = "Hello 世界! Привет мир! 🎉 café naïve résumé";

        let hash = store.write(unicode_content).await.unwrap();
        let read_content = store.read(&hash).await.unwrap();

        assert_eq!(read_content, unicode_content);
    }

    #[tokio::test]
    async fn test_binary_like_content() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        // Content with special characters (but still valid UTF-8)
        let special_content = "\t\n\r\0null byte and special chars";

        let hash = store.write(special_content).await.unwrap();
        let read_content = store.read(&hash).await.unwrap();

        assert_eq!(read_content, special_content);
    }

    #[tokio::test]
    async fn test_json_content() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let json_content = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello"}]}]}"#;

        let hash = store.write(json_content).await.unwrap();
        let read_content = store.read(&hash).await.unwrap();

        assert_eq!(read_content, json_content);

        // Verify it's valid JSON
        let _: serde_json::Value = serde_json::from_str(&read_content).unwrap();
    }

    // ============================================
    // Total size calculation
    // ============================================

    #[tokio::test]
    async fn test_total_size_empty() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let size = store.total_size().await.unwrap();
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_total_size_with_objects() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        // Write some objects
        store.write("content 1").await.unwrap();
        store.write("content 2").await.unwrap();
        store.write("content 3").await.unwrap();

        let size = store.total_size().await.unwrap();
        assert!(size > 0);
    }

    #[tokio::test]
    async fn test_total_size_nonexistent_dir() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        // Don't call init()

        let size = store.total_size().await.unwrap();
        assert_eq!(size, 0);
    }

    // ============================================
    // Garbage collection
    // ============================================

    #[tokio::test]
    async fn test_gc_removes_unreferenced() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        // Write 5 objects
        let hash1 = store.write("content 1").await.unwrap();
        let hash2 = store.write("content 2").await.unwrap();
        let hash3 = store.write("content 3").await.unwrap();
        let hash4 = store.write("content 4").await.unwrap();
        let hash5 = store.write("content 5").await.unwrap();

        // Mark only 2 as used
        let mut used: HashSet<String> = HashSet::new();
        used.insert(hash1.clone());
        used.insert(hash3.clone());

        // Run GC
        let deleted = store.gc(&used).await.unwrap();
        assert_eq!(deleted, 3); // hash2, hash4, hash5

        // Verify used objects still exist
        assert!(store.exists(&hash1).await);
        assert!(store.exists(&hash3).await);

        // Verify unused objects are gone
        assert!(!store.exists(&hash2).await);
        assert!(!store.exists(&hash4).await);
        assert!(!store.exists(&hash5).await);
    }

    #[tokio::test]
    async fn test_gc_preserves_all_when_all_used() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let hash1 = store.write("content 1").await.unwrap();
        let hash2 = store.write("content 2").await.unwrap();

        let mut used: HashSet<String> = HashSet::new();
        used.insert(hash1.clone());
        used.insert(hash2.clone());

        let deleted = store.gc(&used).await.unwrap();
        assert_eq!(deleted, 0);

        assert!(store.exists(&hash1).await);
        assert!(store.exists(&hash2).await);
    }

    #[tokio::test]
    async fn test_gc_empty_store() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let used: HashSet<String> = HashSet::new();
        let deleted = store.gc(&used).await.unwrap();
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn test_gc_nonexistent_dir() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        // Don't call init()

        let used: HashSet<String> = HashSet::new();
        let deleted = store.gc(&used).await.unwrap();
        assert_eq!(deleted, 0);
    }

    // ============================================
    // Object path structure
    // ============================================

    #[tokio::test]
    async fn test_object_path_structure() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let content = "Test content for path structure";
        let hash = store.write(content).await.unwrap();

        // Verify git-like directory structure
        let expected_dir = temp
            .path()
            .join(".midlight")
            .join("objects")
            .join(&hash[..2]);
        let expected_file = expected_dir.join(&hash[2..]);

        assert!(expected_dir.exists());
        assert!(expected_file.exists());
    }

    #[tokio::test]
    async fn test_short_hash_handling() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());

        // Test internal path generation with short hash
        let path = store.get_object_path("a");
        assert!(path.ends_with("a"));

        let path = store.get_object_path("ab");
        assert!(path.to_string_lossy().contains("ab/"));
    }

    // ============================================
    // Multiple objects
    // ============================================

    #[tokio::test]
    async fn test_multiple_objects() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let mut hashes = Vec::new();

        // Write 20 different objects
        for i in 0..20 {
            let content = format!("Content number {}", i);
            let hash = store.write(&content).await.unwrap();
            hashes.push((hash, content));
        }

        // Verify all can be read back
        for (hash, expected_content) in &hashes {
            let read_content = store.read(hash).await.unwrap();
            assert_eq!(&read_content, expected_content);
        }
    }

    // ============================================
    // ObjectStoreOps trait implementation
    // ============================================

    #[tokio::test]
    async fn test_trait_write_and_read() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        ObjectStoreOps::init(&store).await.unwrap();

        let content = "Test via trait";
        let hash = ObjectStoreOps::write(&store, content).await.unwrap();
        let read_content = ObjectStoreOps::read(&store, &hash).await.unwrap();

        assert_eq!(read_content, content);
    }

    #[tokio::test]
    async fn test_trait_exists() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        ObjectStoreOps::init(&store).await.unwrap();

        let hash = ObjectStoreOps::write(&store, "test").await.unwrap();

        assert!(ObjectStoreOps::exists(&store, &hash).await);
        assert!(!ObjectStoreOps::exists(&store, "nonexistent").await);
    }

    #[tokio::test]
    async fn test_trait_delete() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        ObjectStoreOps::init(&store).await.unwrap();

        let hash = ObjectStoreOps::write(&store, "test").await.unwrap();
        assert!(ObjectStoreOps::exists(&store, &hash).await);

        ObjectStoreOps::delete(&store, &hash).await.unwrap();
        assert!(!ObjectStoreOps::exists(&store, &hash).await);
    }

    #[tokio::test]
    async fn test_trait_delete_nonexistent() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        ObjectStoreOps::init(&store).await.unwrap();

        let result = ObjectStoreOps::delete(&store, "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trait_read_nonexistent() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        ObjectStoreOps::init(&store).await.unwrap();

        let result = ObjectStoreOps::read(&store, "nonexistent").await;
        assert!(result.is_err());

        // Verify it's a NotFound error
        if let Err(ObjectStoreError::NotFound(_)) = result {
            // Expected
        } else {
            panic!("Expected NotFound error");
        }
    }

    // ============================================
    // Empty content handling
    // ============================================

    #[tokio::test]
    async fn test_empty_content() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        let hash = store.write("").await.unwrap();
        let read_content = store.read(&hash).await.unwrap();

        assert_eq!(read_content, "");
    }

    // ============================================
    // Corrupted file handling
    // ============================================

    #[tokio::test]
    async fn test_corrupted_object() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        // Write valid object to get the path
        let hash = store.write("valid content").await.unwrap();
        let object_path = temp
            .path()
            .join(".midlight")
            .join("objects")
            .join(&hash[..2])
            .join(&hash[2..]);

        // Corrupt the file with invalid gzip data
        std::fs::write(&object_path, b"not valid gzip data").unwrap();

        // Reading should fail
        let result = store.read(&hash).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_truncated_compressed_data() {
        let temp = tempdir().unwrap();
        let store = ObjectStore::new(temp.path());
        store.init().await.unwrap();

        // Write valid object to get the path
        let hash = store
            .write("valid content that will be truncated")
            .await
            .unwrap();
        let object_path = temp
            .path()
            .join(".midlight")
            .join("objects")
            .join(&hash[..2])
            .join(&hash[2..]);

        // Read the file and truncate it
        let data = std::fs::read(&object_path).unwrap();
        std::fs::write(&object_path, &data[..data.len() / 2]).unwrap();

        // Reading should fail
        let result = store.read(&hash).await;
        assert!(result.is_err());
    }
}
