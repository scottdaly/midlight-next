// Vector Store - SQLite-based vector storage with cosine similarity search
//
// Stores document chunks with their embeddings and provides semantic search
// using cosine similarity.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

// ============================================================================
// Types
// ============================================================================

/// A stored document chunk with its embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredChunk {
    pub id: String,
    pub project_path: String,
    pub file_path: String,
    pub chunk_index: i32,
    pub content: String,
    pub heading: Option<String>,
    pub embedding: Vec<f32>,
    pub created_at: String,
}

/// Index status for a project
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub project_path: String,
    pub project_name: Option<String>,
    pub total_documents: u32,
    pub indexed_documents: u32,
    pub total_chunks: u32,
    pub last_indexed: Option<String>,
    pub is_indexing: bool,
    pub error: Option<String>,
}

impl Default for IndexStatus {
    fn default() -> Self {
        Self {
            project_path: String::new(),
            project_name: None,
            total_documents: 0,
            indexed_documents: 0,
            total_chunks: 0,
            last_indexed: None,
            is_indexing: false,
            error: None,
        }
    }
}

/// Search result with score
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub chunk: DocumentChunk,
    pub score: f32,
}

/// Document chunk metadata for search results (without embedding)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChunk {
    pub id: String,
    pub project_path: String,
    pub file_path: String,
    pub chunk_index: i32,
    pub content: String,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkMetadata {
    pub heading: Option<String>,
    pub section: Option<String>,
    pub token_estimate: u32,
}

// ============================================================================
// Vector Store
// ============================================================================

pub struct VectorStore {
    conn: Arc<Mutex<Connection>>,
}

impl VectorStore {
    /// Create a new vector store, initializing the database if needed
    pub fn new(db_path: PathBuf) -> Result<Self, String> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create database directory: {}", e))?;
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("Failed to set pragmas: {}", e))?;

        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS document_chunks (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                heading TEXT,
                embedding BLOB NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(project_path, file_path, chunk_index)
            )",
            [],
        )
        .map_err(|e| format!("Failed to create table: {}", e))?;

        // Create indexes for efficient queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_project ON document_chunks(project_path)",
            [],
        )
        .ok();

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_file ON document_chunks(project_path, file_path)",
            [],
        )
        .ok();

        info!("Vector store initialized at {:?}", db_path);

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Upsert chunks into the vector store
    pub async fn upsert_chunks(&self, chunks: Vec<StoredChunk>) -> Result<usize, String> {
        if chunks.is_empty() {
            return Ok(0);
        }

        let conn = self.conn.lock().await;

        let mut count = 0;
        for chunk in &chunks {
            // Convert embedding to bytes
            let embedding_bytes: Vec<u8> = chunk
                .embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();

            let result = conn.execute(
                "INSERT OR REPLACE INTO document_chunks
                 (id, project_path, file_path, chunk_index, content, heading, embedding, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    chunk.id,
                    chunk.project_path,
                    chunk.file_path,
                    chunk.chunk_index,
                    chunk.content,
                    chunk.heading,
                    embedding_bytes,
                    chunk.created_at,
                ],
            );

            match result {
                Ok(_) => count += 1,
                Err(e) => error!("Failed to insert chunk {}: {}", chunk.id, e),
            }
        }

        debug!("Upserted {} chunks", count);
        Ok(count)
    }

    /// Search for chunks similar to the query embedding
    pub async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        project_filter: Option<&[String]>,
        min_score: Option<f32>,
    ) -> Result<Vec<SearchResult>, String> {
        let conn = self.conn.lock().await;

        // Build query with optional project filter
        let sql = match project_filter {
            Some(projects) if !projects.is_empty() => {
                let placeholders: Vec<&str> = projects.iter().map(|_| "?").collect();
                format!(
                    "SELECT id, project_path, file_path, chunk_index, content, heading, embedding, created_at
                     FROM document_chunks
                     WHERE project_path IN ({})",
                    placeholders.join(",")
                )
            }
            _ => {
                "SELECT id, project_path, file_path, chunk_index, content, heading, embedding, created_at
                 FROM document_chunks".to_string()
            }
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        // Bind project filter parameters if present
        let rows = if let Some(projects) = project_filter {
            let params: Vec<&dyn rusqlite::ToSql> = projects
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();
            stmt.query(params.as_slice())
        } else {
            stmt.query([])
        };

        let mut rows = rows.map_err(|e| format!("Query failed: {}", e))?;

        // Collect all chunks and compute similarity
        let mut results: Vec<(SearchResult, f32)> = Vec::new();
        let threshold = min_score.unwrap_or(0.0);

        while let Some(row) = rows.next().map_err(|e| format!("Row error: {}", e))? {
            let embedding_blob: Vec<u8> = row.get(6).map_err(|e| format!("Get embedding: {}", e))?;

            // Convert bytes back to f32 vec
            let embedding: Vec<f32> = embedding_blob
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            let score = cosine_similarity(query_embedding, &embedding);

            if score >= threshold {
                let heading: Option<String> = row.get(5).ok();
                let content: String = row.get(4).map_err(|e| format!("Get content: {}", e))?;

                let result = SearchResult {
                    chunk: DocumentChunk {
                        id: row.get(0).map_err(|e| format!("Get id: {}", e))?,
                        project_path: row.get(1).map_err(|e| format!("Get project_path: {}", e))?,
                        file_path: row.get(2).map_err(|e| format!("Get file_path: {}", e))?,
                        chunk_index: row.get(3).map_err(|e| format!("Get chunk_index: {}", e))?,
                        content: content.clone(),
                        metadata: ChunkMetadata {
                            heading,
                            section: None,
                            token_estimate: (content.len() / 4) as u32,
                        },
                    },
                    score,
                };
                results.push((result, score));
            }
        }

        // Sort by score descending and take top_k
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        Ok(results.into_iter().map(|(r, _)| r).collect())
    }

    /// Delete all chunks for a project
    pub async fn delete_project(&self, project_path: &str) -> Result<usize, String> {
        let conn = self.conn.lock().await;

        let deleted = conn
            .execute(
                "DELETE FROM document_chunks WHERE project_path = ?1",
                params![project_path],
            )
            .map_err(|e| format!("Delete failed: {}", e))?;

        info!("Deleted {} chunks for project {}", deleted, project_path);
        Ok(deleted)
    }

    /// Delete chunks for a specific file
    pub async fn delete_file(&self, project_path: &str, file_path: &str) -> Result<usize, String> {
        let conn = self.conn.lock().await;

        let deleted = conn
            .execute(
                "DELETE FROM document_chunks WHERE project_path = ?1 AND file_path = ?2",
                params![project_path, file_path],
            )
            .map_err(|e| format!("Delete failed: {}", e))?;

        debug!("Deleted {} chunks for file {}", deleted, file_path);
        Ok(deleted)
    }

    /// Get index status for projects
    pub async fn get_status(&self, project_path: Option<&str>) -> Result<Vec<IndexStatus>, String> {
        let conn = self.conn.lock().await;

        let sql = match project_path {
            Some(_) => {
                "SELECT project_path,
                        COUNT(DISTINCT file_path) as doc_count,
                        COUNT(*) as chunk_count,
                        MAX(created_at) as last_indexed
                 FROM document_chunks
                 WHERE project_path = ?1
                 GROUP BY project_path"
            }
            None => {
                "SELECT project_path,
                        COUNT(DISTINCT file_path) as doc_count,
                        COUNT(*) as chunk_count,
                        MAX(created_at) as last_indexed
                 FROM document_chunks
                 GROUP BY project_path"
            }
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let rows = if let Some(path) = project_path {
            stmt.query(params![path])
        } else {
            stmt.query([])
        };

        let mut rows = rows.map_err(|e| format!("Query failed: {}", e))?;
        let mut statuses = Vec::new();

        while let Some(row) = rows.next().map_err(|e| format!("Row error: {}", e))? {
            let project_path: String = row.get(0).map_err(|e| format!("Get project_path: {}", e))?;
            let doc_count: u32 = row.get(1).map_err(|e| format!("Get doc_count: {}", e))?;
            let chunk_count: u32 = row.get(2).map_err(|e| format!("Get chunk_count: {}", e))?;
            let last_indexed: Option<String> = row.get(3).ok();

            // Extract project name from path
            let project_name = std::path::Path::new(&project_path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from);

            statuses.push(IndexStatus {
                project_path,
                project_name,
                total_documents: doc_count,
                indexed_documents: doc_count,
                total_chunks: chunk_count,
                last_indexed,
                is_indexing: false,
                error: None,
            });
        }

        Ok(statuses)
    }

    /// Get total chunk count
    pub async fn get_chunk_count(&self) -> Result<u32, String> {
        let conn = self.conn.lock().await;

        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM document_chunks", [], |row| row.get(0))
            .map_err(|e| format!("Count failed: {}", e))?;

        Ok(count)
    }

    /// Check if a file has been indexed
    pub async fn is_file_indexed(&self, project_path: &str, file_path: &str) -> Result<bool, String> {
        let conn = self.conn.lock().await;

        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM document_chunks WHERE project_path = ?1 AND file_path = ?2",
                params![project_path, file_path],
                |row| row.get(0),
            )
            .map_err(|e| format!("Query failed: {}", e))?;

        Ok(count > 0)
    }
}

// ============================================================================
// Cosine Similarity
// ============================================================================

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_chunk(id: &str, content: &str, embedding: Vec<f32>) -> StoredChunk {
        StoredChunk {
            id: id.to_string(),
            project_path: "/test/project".to_string(),
            file_path: "test.md".to_string(),
            chunk_index: 0,
            content: content.to_string(),
            heading: Some("Test Heading".to_string()),
            embedding,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn test_create_vector_store() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let store = VectorStore::new(db_path);
        assert!(store.is_ok());
    }

    #[tokio::test]
    async fn test_upsert_and_search() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = VectorStore::new(db_path).unwrap();

        // Create test embeddings (3-dimensional for simplicity)
        let chunk1 = create_test_chunk("1", "Hello world", vec![1.0, 0.0, 0.0]);
        let chunk2 = create_test_chunk("2", "Goodbye world", vec![0.0, 1.0, 0.0]);
        let chunk3 = create_test_chunk("3", "Hello there", vec![0.9, 0.1, 0.0]);

        // Upsert chunks
        let count = store
            .upsert_chunks(vec![chunk1, chunk2, chunk3])
            .await
            .unwrap();
        assert_eq!(count, 3);

        // Search for similar to [1.0, 0.0, 0.0]
        let results = store
            .search(&[1.0, 0.0, 0.0], 2, None, None)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        // chunk1 should be first (exact match), chunk3 should be second (high similarity)
        assert_eq!(results[0].chunk.id, "1");
        assert!(results[0].score > 0.99);
    }

    #[tokio::test]
    async fn test_delete_project() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = VectorStore::new(db_path).unwrap();

        let chunk = create_test_chunk("1", "Test content", vec![1.0, 0.0, 0.0]);
        store.upsert_chunks(vec![chunk]).await.unwrap();

        let count = store.get_chunk_count().await.unwrap();
        assert_eq!(count, 1);

        store.delete_project("/test/project").await.unwrap();

        let count = store.get_chunk_count().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_status() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = VectorStore::new(db_path).unwrap();

        let chunk = create_test_chunk("1", "Test content", vec![1.0, 0.0, 0.0]);
        store.upsert_chunks(vec![chunk]).await.unwrap();

        let statuses = store.get_status(None).await.unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].total_chunks, 1);
    }

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 0.001);

        // Orthogonal vectors
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 0.001);

        // Opposite vectors
        assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 0.001);

        // Empty or mismatched vectors
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }
}
