// Image manager - Content-addressable image storage with deduplication

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use super::error::{MidlightError, Result};

/// Manages image storage for a workspace
pub struct ImageManager {
    images_dir: PathBuf,
}

impl ImageManager {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            images_dir: workspace_root.join(".midlight").join("images"),
        }
    }

    /// Initialize the image manager
    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.images_dir)?;
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
        if !file_path.exists() {
            fs::write(&file_path, &image_data)?;
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
        let matching_file = self.find_image_by_hash(hash)?;

        // Read file
        let image_data = fs::read(&matching_file)?;

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
    pub fn exists(&self, ref_id: &str) -> bool {
        let hash = ref_id.strip_prefix("midlight://img-").unwrap_or(ref_id);
        self.find_image_by_hash(hash).is_ok()
    }

    /// Delete an image
    pub async fn delete(&self, ref_id: &str) -> Result<()> {
        let hash = ref_id.strip_prefix("midlight://img-").unwrap_or(ref_id);
        let file_path = self.find_image_by_hash(hash)?;
        fs::remove_file(&file_path)?;
        tracing::debug!("Deleted image: {}", file_path.display());
        Ok(())
    }

    /// List all images
    pub async fn list_images(&self) -> Result<Vec<String>> {
        let mut images = Vec::new();

        if self.images_dir.exists() {
            for entry in fs::read_dir(&self.images_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        images.push(format!("midlight://img-{}", stem));
                    }
                }
            }
        }

        Ok(images)
    }

    /// Find image file by hash prefix
    fn find_image_by_hash(&self, hash: &str) -> Result<PathBuf> {
        if !self.images_dir.exists() {
            return Err(MidlightError::NotFound(format!(
                "Image not found: {}",
                hash
            )));
        }

        for entry in fs::read_dir(&self.images_dir)? {
            let entry = entry?;
            let path = entry.path();
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
