// RAG Service - Retrieval Augmented Generation orchestration
//
// Coordinates document indexing and semantic search:
// 1. Scans project for .midlight files
// 2. Chunks documents into smaller pieces
// 3. Generates embeddings via the embedding service
// 4. Stores in vector database
// 5. Retrieves relevant chunks for queries

use crate::services::embedding_service::EmbeddingService;
use crate::services::vector_store::{IndexStatus, SearchResult, StoredChunk, VectorStore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

// ============================================================================
// Configuration
// ============================================================================

/// Maximum characters per chunk (roughly 500 tokens)
const MAX_CHUNK_SIZE: usize = 2000;

/// Minimum characters per chunk (avoid tiny chunks)
const MIN_CHUNK_SIZE: usize = 100;

/// File extensions to index
const INDEXABLE_EXTENSIONS: &[&str] = &["midlight", "md", "txt"];

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    /// Maximum number of results to return
    pub top_k: Option<u32>,
    /// Minimum similarity score (0.0 - 1.0)
    pub min_score: Option<f32>,
    /// Filter by project paths
    pub project_paths: Option<Vec<String>>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            top_k: Some(5),
            min_score: Some(0.3),
            project_paths: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RAGError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for RAGError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RAGError {}

// ============================================================================
// RAG Service
// ============================================================================

pub struct RAGService {
    vector_store: Arc<VectorStore>,
    embedding_service: Arc<EmbeddingService>,
    /// Track which projects are currently being indexed
    indexing_projects: Arc<RwLock<HashSet<String>>>,
}

impl RAGService {
    /// Create a new RAG service
    pub fn new(db_path: PathBuf) -> Result<Self, String> {
        let vector_store = Arc::new(VectorStore::new(db_path)?);
        let embedding_service = Arc::new(EmbeddingService::default());

        Ok(Self {
            vector_store,
            embedding_service,
            indexing_projects: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    /// Index a project for semantic search
    ///
    /// # Arguments
    /// * `project_path` - Path to the project root
    /// * `auth_token` - User's authentication token
    /// * `force` - If true, re-index even if already indexed
    pub async fn index_project(
        &self,
        project_path: &str,
        auth_token: &str,
        force: bool,
    ) -> Result<IndexStatus, RAGError> {
        // Check if already indexing
        {
            let indexing = self.indexing_projects.read().await;
            if indexing.contains(project_path) {
                return Err(RAGError {
                    code: "ALREADY_INDEXING".to_string(),
                    message: format!("Project {} is already being indexed", project_path),
                });
            }
        }

        // Mark as indexing
        {
            let mut indexing = self.indexing_projects.write().await;
            indexing.insert(project_path.to_string());
        }

        let result = self
            .do_index_project(project_path, auth_token, force)
            .await;

        // Remove from indexing set
        {
            let mut indexing = self.indexing_projects.write().await;
            indexing.remove(project_path);
        }

        result
    }

    /// Internal implementation of index_project
    async fn do_index_project(
        &self,
        project_path: &str,
        auth_token: &str,
        force: bool,
    ) -> Result<IndexStatus, RAGError> {
        info!("Indexing project: {}", project_path);

        // Delete existing chunks if force re-index
        if force {
            self.vector_store
                .delete_project(project_path)
                .await
                .map_err(|e| RAGError {
                    code: "DELETE_ERROR".to_string(),
                    message: e,
                })?;
        }

        // Scan for files to index
        let files = self.scan_project_files(project_path)?;
        info!("Found {} files to index", files.len());

        if files.is_empty() {
            return Ok(IndexStatus {
                project_path: project_path.to_string(),
                project_name: Path::new(project_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(String::from),
                total_documents: 0,
                indexed_documents: 0,
                total_chunks: 0,
                last_indexed: Some(chrono::Utc::now().to_rfc3339()),
                is_indexing: false,
                error: None,
            });
        }

        // Process each file
        let mut all_chunks: Vec<(String, String, String)> = Vec::new(); // (id, content, file_path)
        let mut indexed_files = 0;

        for file_path in &files {
            match self.process_file(project_path, file_path) {
                Ok(chunks) => {
                    all_chunks.extend(chunks);
                    indexed_files += 1;
                }
                Err(e) => {
                    warn!("Failed to process file {}: {}", file_path, e);
                }
            }
        }

        if all_chunks.is_empty() {
            return Ok(IndexStatus {
                project_path: project_path.to_string(),
                project_name: Path::new(project_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(String::from),
                total_documents: files.len() as u32,
                indexed_documents: 0,
                total_chunks: 0,
                last_indexed: Some(chrono::Utc::now().to_rfc3339()),
                is_indexing: false,
                error: Some("No content to index".to_string()),
            });
        }

        // Generate embeddings in batches
        let texts: Vec<String> = all_chunks.iter().map(|(_, c, _)| c.clone()).collect();
        let embeddings = self
            .embedding_service
            .embed_texts(texts, auth_token)
            .await
            .map_err(|e| RAGError {
                code: e.code,
                message: e.message,
            })?;

        // Create stored chunks
        let timestamp = chrono::Utc::now().to_rfc3339();
        let stored_chunks: Vec<StoredChunk> = all_chunks
            .into_iter()
            .zip(embeddings)
            .enumerate()
            .map(|(i, ((id, content, file_path), embedding))| StoredChunk {
                id,
                project_path: project_path.to_string(),
                file_path,
                chunk_index: i as i32,
                content,
                heading: None, // TODO: Extract headings
                embedding,
                created_at: timestamp.clone(),
            })
            .collect();

        let chunk_count = stored_chunks.len();

        // Store in vector database
        self.vector_store
            .upsert_chunks(stored_chunks)
            .await
            .map_err(|e| RAGError {
                code: "STORE_ERROR".to_string(),
                message: e,
            })?;

        info!(
            "Indexed {} files with {} chunks for project {}",
            indexed_files, chunk_count, project_path
        );

        Ok(IndexStatus {
            project_path: project_path.to_string(),
            project_name: Path::new(project_path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from),
            total_documents: files.len() as u32,
            indexed_documents: indexed_files,
            total_chunks: chunk_count as u32,
            last_indexed: Some(timestamp),
            is_indexing: false,
            error: None,
        })
    }

    /// Search for relevant chunks
    pub async fn search(
        &self,
        query: &str,
        auth_token: &str,
        options: Option<SearchOptions>,
    ) -> Result<Vec<SearchResult>, RAGError> {
        let opts = options.unwrap_or_default();

        // Generate query embedding
        let query_embedding = self
            .embedding_service
            .embed_query(query, auth_token)
            .await
            .map_err(|e| RAGError {
                code: e.code,
                message: e.message,
            })?;

        // Search vector store
        let results = self
            .vector_store
            .search(
                &query_embedding,
                opts.top_k.unwrap_or(5) as usize,
                opts.project_paths.as_deref(),
                opts.min_score,
            )
            .await
            .map_err(|e| RAGError {
                code: "SEARCH_ERROR".to_string(),
                message: e,
            })?;

        debug!("Found {} results for query: {}", results.len(), query);
        Ok(results)
    }

    /// Get index status for projects
    pub async fn get_status(
        &self,
        project_path: Option<&str>,
    ) -> Result<Vec<IndexStatus>, RAGError> {
        let mut statuses = self
            .vector_store
            .get_status(project_path)
            .await
            .map_err(|e| RAGError {
                code: "STATUS_ERROR".to_string(),
                message: e,
            })?;

        // Mark currently indexing projects
        let indexing = self.indexing_projects.read().await;
        for status in &mut statuses {
            if indexing.contains(&status.project_path) {
                status.is_indexing = true;
            }
        }

        Ok(statuses)
    }

    /// Delete index for a project
    pub async fn delete_index(&self, project_path: &str) -> Result<(), RAGError> {
        self.vector_store
            .delete_project(project_path)
            .await
            .map_err(|e| RAGError {
                code: "DELETE_ERROR".to_string(),
                message: e,
            })?;

        info!("Deleted index for project: {}", project_path);
        Ok(())
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    /// Scan project for indexable files
    fn scan_project_files(&self, project_path: &str) -> Result<Vec<String>, RAGError> {
        let mut files = Vec::new();

        for entry in WalkDir::new(project_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip hidden directories and files
            if path
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            {
                continue;
            }

            // Skip non-files
            if !path.is_file() {
                continue;
            }

            // Check extension
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if INDEXABLE_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }

        Ok(files)
    }

    /// Process a single file into chunks
    fn process_file(
        &self,
        project_path: &str,
        file_path: &str,
    ) -> Result<Vec<(String, String, String)>, String> {
        let content =
            std::fs::read_to_string(file_path).map_err(|e| format!("Read error: {}", e))?;

        if content.trim().is_empty() {
            return Ok(vec![]);
        }

        // Get relative path for storage
        let relative_path = Path::new(file_path)
            .strip_prefix(project_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_path.to_string());

        // Chunk the content
        let chunks = self.chunk_content(&content);

        // Create chunk IDs and tuples
        let result: Vec<(String, String, String)> = chunks
            .into_iter()
            .enumerate()
            .map(|(i, chunk)| {
                let id = format!("{}:{}:{}", project_path, relative_path, i);
                (id, chunk, relative_path.clone())
            })
            .collect();

        debug!(
            "Processed {} into {} chunks",
            relative_path,
            result.len()
        );
        Ok(result)
    }

    /// Chunk content into smaller pieces for embedding
    fn chunk_content(&self, content: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        // Split by paragraphs (double newline)
        for paragraph in content.split("\n\n") {
            let trimmed = paragraph.trim();
            if trimmed.is_empty() {
                continue;
            }

            // If adding this paragraph exceeds max size, save current and start new
            if !current_chunk.is_empty()
                && current_chunk.len() + trimmed.len() + 2 > MAX_CHUNK_SIZE
            {
                if current_chunk.len() >= MIN_CHUNK_SIZE {
                    chunks.push(current_chunk.clone());
                }
                current_chunk.clear();
            }

            // Add paragraph to current chunk
            if !current_chunk.is_empty() {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(trimmed);

            // If current chunk is already at max, save it
            if current_chunk.len() >= MAX_CHUNK_SIZE {
                chunks.push(current_chunk.clone());
                current_chunk.clear();
            }
        }

        // Don't forget the last chunk
        if !current_chunk.is_empty() && current_chunk.len() >= MIN_CHUNK_SIZE {
            chunks.push(current_chunk);
        } else if !current_chunk.is_empty() && chunks.is_empty() {
            // If this is the only content and it's small, still include it
            chunks.push(current_chunk);
        }

        chunks
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_service() -> RAGService {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        RAGService::new(db_path).unwrap()
    }

    #[test]
    fn test_chunk_content_single_paragraph() {
        let service = create_test_service();
        let content = "This is a single paragraph.";

        let chunks = service.chunk_content(content);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "This is a single paragraph.");
    }

    #[test]
    fn test_chunk_content_multiple_paragraphs() {
        let service = create_test_service();
        let content = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";

        let chunks = service.chunk_content(content);

        assert_eq!(chunks.len(), 1); // All fit in one chunk
        assert!(chunks[0].contains("First"));
        assert!(chunks[0].contains("Second"));
        assert!(chunks[0].contains("Third"));
    }

    #[test]
    fn test_chunk_content_large_content() {
        let service = create_test_service();

        // Create content larger than MAX_CHUNK_SIZE
        let large_para = "x".repeat(1500);
        let content = format!("{}\n\n{}\n\n{}", large_para, large_para, large_para);

        let chunks = service.chunk_content(&content);

        // Should be split into multiple chunks
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_chunk_content_empty() {
        let service = create_test_service();
        let content = "";

        let chunks = service.chunk_content(content);

        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_content_whitespace_only() {
        let service = create_test_service();
        let content = "   \n\n   \n\n   ";

        let chunks = service.chunk_content(content);

        assert!(chunks.is_empty());
    }

    #[test]
    fn test_search_options_default() {
        let opts = SearchOptions::default();

        assert_eq!(opts.top_k, Some(5));
        assert_eq!(opts.min_score, Some(0.3));
        assert!(opts.project_paths.is_none());
    }

    #[test]
    fn test_rag_error_display() {
        let error = RAGError {
            code: "TEST_ERROR".to_string(),
            message: "Something went wrong".to_string(),
        };

        assert_eq!(format!("{}", error), "TEST_ERROR: Something went wrong");
    }
}
