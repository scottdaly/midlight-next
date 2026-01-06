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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
}
