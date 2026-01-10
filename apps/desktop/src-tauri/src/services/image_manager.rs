// Image manager - Content-addressable image storage with deduplication

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::error::{MidlightError, Result};
use crate::traits::{FileSystem, TokioFileSystem};

/// Manages image storage for a workspace
pub struct ImageManager<F: FileSystem = TokioFileSystem> {
    images_dir: PathBuf,
    fs: Arc<F>,
}

/// Type alias for production use
#[allow(dead_code)]
pub type ProductionImageManager = ImageManager<TokioFileSystem>;

impl ImageManager<TokioFileSystem> {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            images_dir: workspace_root.join(".midlight").join("images"),
            fs: Arc::new(TokioFileSystem::new()),
        }
    }
}

impl<F: FileSystem> ImageManager<F> {
    /// Create a new ImageManager with custom dependencies (for testing)
    #[allow(dead_code)]
    pub fn with_fs(workspace_root: &Path, fs: Arc<F>) -> Self {
        Self {
            images_dir: workspace_root.join(".midlight").join("images"),
            fs,
        }
    }

    /// Initialize the image manager
    pub async fn init(&self) -> Result<()> {
        self.fs.create_dir_all(&self.images_dir).await?;
        Ok(())
    }

    /// Store an image from a data URL, returns the image reference ID
    /// Format: "midlight://img-{hash}"
    pub async fn store_image(
        &self,
        data_url: &str,
        _original_name: Option<&str>,
    ) -> Result<String> {
        // Parse data URL: data:image/png;base64,iVBORw0KGgo...
        let parts: Vec<&str> = data_url.splitn(2, ',').collect();
        if parts.len() != 2 {
            return Err(MidlightError::InvalidInput(
                "Invalid data URL format".to_string(),
            ));
        }

        let header = parts[0];
        let base64_data = parts[1];

        // Extract mime type
        let mime_type = header
            .strip_prefix("data:")
            .and_then(|s| s.split(';').next())
            .unwrap_or("image/png");

        // Decode base64
        let image_data = BASE64
            .decode(base64_data)
            .map_err(|e| MidlightError::InvalidInput(format!("Invalid base64: {}", e)))?;

        // Calculate SHA-256 hash for deduplication
        let mut hasher = Sha256::new();
        hasher.update(&image_data);
        let hash = format!("{:x}", hasher.finalize());
        let short_hash = &hash[..16];

        // Determine extension from mime type
        let extension = match mime_type {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            _ => "bin",
        };

        // Create filename with hash
        let filename = format!("{}.{}", short_hash, extension);
        let file_path = self.images_dir.join(&filename);

        // Only write if doesn't exist (deduplication)
        if !self.fs.exists(&file_path).await {
            self.fs.write_bytes(&file_path, &image_data).await?;
            tracing::debug!(
                "Stored new image: {} ({} bytes)",
                filename,
                image_data.len()
            );
        } else {
            tracing::debug!("Image already exists: {}", filename);
        }

        // Return reference ID
        Ok(format!("midlight://img-{}", short_hash))
    }

    /// Get an image as a data URL
    pub async fn get_image_data_url(&self, ref_id: &str) -> Result<String> {
        // Parse reference: "midlight://img-{hash}" or just the hash
        let hash = ref_id.strip_prefix("midlight://img-").unwrap_or(ref_id);

        // Find the image file (any extension)
        let matching_file = self.find_image_by_hash(hash).await?;

        // Read file
        let image_data = self.fs.read(&matching_file).await?;

        // Determine mime type from extension
        let mime_type = matching_file
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| match ext {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "svg" => "image/svg+xml",
                _ => "application/octet-stream",
            })
            .unwrap_or("application/octet-stream");

        // Encode as data URL
        let base64_data = BASE64.encode(&image_data);
        Ok(format!("data:{};base64,{}", mime_type, base64_data))
    }

    /// Check if an image exists
    pub async fn exists(&self, ref_id: &str) -> bool {
        let hash = ref_id.strip_prefix("midlight://img-").unwrap_or(ref_id);
        self.find_image_by_hash(hash).await.is_ok()
    }

    /// Delete an image
    pub async fn delete(&self, ref_id: &str) -> Result<()> {
        let hash = ref_id.strip_prefix("midlight://img-").unwrap_or(ref_id);
        let file_path = self.find_image_by_hash(hash).await?;
        self.fs.remove_file(&file_path).await?;
        tracing::debug!("Deleted image: {}", file_path.display());
        Ok(())
    }

    /// List all images
    pub async fn list_images(&self) -> Result<Vec<String>> {
        let mut images = Vec::new();

        if self.fs.exists(&self.images_dir).await {
            let entries = self.fs.read_dir(&self.images_dir).await?;
            for path in entries {
                if self.fs.is_file(&path).await {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        images.push(format!("midlight://img-{}", stem));
                    }
                }
            }
        }

        Ok(images)
    }

    /// Find image file by hash prefix
    async fn find_image_by_hash(&self, hash: &str) -> Result<PathBuf> {
        if !self.fs.exists(&self.images_dir).await {
            return Err(MidlightError::NotFound(format!(
                "Image not found: {}",
                hash
            )));
        }

        let entries = self.fs.read_dir(&self.images_dir).await?;
        for path in entries {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem == hash || stem.starts_with(hash) {
                    return Ok(path);
                }
            }
        }

        Err(MidlightError::NotFound(format!(
            "Image not found: {}",
            hash
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::file_system::MockFileSystem;

    // A simple 1x1 red PNG image
    const TINY_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

    fn create_test_manager() -> ImageManager<MockFileSystem> {
        let fs = Arc::new(MockFileSystem::new());
        ImageManager::with_fs(Path::new("/workspace"), fs)
    }

    fn create_png_data_url() -> String {
        format!("data:image/png;base64,{}", TINY_PNG_BASE64)
    }

    #[tokio::test]
    async fn test_init_creates_images_dir() {
        let fs = Arc::new(MockFileSystem::new());
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs.clone());

        manager.init().await.unwrap();

        // create_dir_all registers the directory in the mock
        assert!(fs.exists(Path::new("/workspace/.midlight/images")).await);
    }

    #[tokio::test]
    async fn test_store_image() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs.clone());

        let data_url = create_png_data_url();
        let ref_id = manager.store_image(&data_url, None).await.unwrap();

        assert!(ref_id.starts_with("midlight://img-"));
        // Hash should be 16 characters
        let hash = ref_id.strip_prefix("midlight://img-").unwrap();
        assert_eq!(hash.len(), 16);
    }

    #[tokio::test]
    async fn test_store_image_deduplication() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs);

        let data_url = create_png_data_url();

        // Store the same image twice
        let ref1 = manager.store_image(&data_url, None).await.unwrap();
        let ref2 = manager.store_image(&data_url, None).await.unwrap();

        // Should get the same reference ID (content-addressable)
        assert_eq!(ref1, ref2);
    }

    #[tokio::test]
    async fn test_store_image_invalid_data_url() {
        let manager = create_test_manager();

        let result = manager.store_image("not a data url", None).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid data URL format"));
    }

    #[tokio::test]
    async fn test_store_image_invalid_base64() {
        let manager = create_test_manager();

        let result = manager
            .store_image("data:image/png;base64,!!!invalid!!!", None)
            .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid base64"));
    }

    #[tokio::test]
    async fn test_get_image_data_url() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs);

        // Store an image first
        let original_data_url = create_png_data_url();
        let ref_id = manager.store_image(&original_data_url, None).await.unwrap();

        // Retrieve it
        let retrieved_data_url = manager.get_image_data_url(&ref_id).await.unwrap();

        // Should be a valid data URL with same content
        assert!(retrieved_data_url.starts_with("data:image/png;base64,"));
    }

    #[tokio::test]
    async fn test_get_image_not_found() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs);

        let result = manager.get_image_data_url("nonexistent").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_exists() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs);

        // Initially doesn't exist
        assert!(!manager.exists("nonexistent").await);

        // Store an image
        let data_url = create_png_data_url();
        let ref_id = manager.store_image(&data_url, None).await.unwrap();

        // Now it exists
        assert!(manager.exists(&ref_id).await);
    }

    #[tokio::test]
    async fn test_delete() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs);

        // Store an image
        let data_url = create_png_data_url();
        let ref_id = manager.store_image(&data_url, None).await.unwrap();

        // Delete it
        manager.delete(&ref_id).await.unwrap();

        // Should no longer exist
        assert!(!manager.exists(&ref_id).await);
    }

    #[tokio::test]
    async fn test_list_images() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs);

        // Initially empty
        let images = manager.list_images().await.unwrap();
        assert!(images.is_empty());

        // Store an image
        let data_url = create_png_data_url();
        let ref_id = manager.store_image(&data_url, None).await.unwrap();

        // Now has one image
        let images = manager.list_images().await.unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0], ref_id);
    }

    #[tokio::test]
    async fn test_mime_type_detection() {
        let fs = Arc::new(MockFileSystem::new().with_dir("/workspace/.midlight/images"));
        let manager = ImageManager::with_fs(Path::new("/workspace"), fs);

        // Test JPEG
        let jpeg_data_url = format!("data:image/jpeg;base64,{}", TINY_PNG_BASE64);
        let ref_id = manager.store_image(&jpeg_data_url, None).await.unwrap();

        // When we retrieve, it should return as JPEG
        let retrieved = manager.get_image_data_url(&ref_id).await.unwrap();
        assert!(retrieved.starts_with("data:image/jpeg;base64,"));
    }
}
