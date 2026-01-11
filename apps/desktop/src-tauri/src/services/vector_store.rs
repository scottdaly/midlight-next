// Vector Store - SQLite-based vector storage with cosine similarity search
//
// Stores document chunks with their embeddings and provides semantic search
// using cosine similarity.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub chunk: DocumentChunk,
    pub score: f32,
}

/// Document chunk metadata for search results (without embedding)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChunk {
    pub id: String,
    pub project_path: String,
    pub file_path: String,
    pub chunk_index: i32,
    pub content: String,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

        // Create indexed_files table for tracking file modification times (incremental indexing)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS indexed_files (
                project_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                indexed_at TEXT NOT NULL,
                chunk_count INTEGER NOT NULL,
                PRIMARY KEY (project_path, file_path)
            )",
            [],
        )
        .map_err(|e| format!("Failed to create indexed_files table: {}", e))?;

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
    ///
    /// Uses a bounded min-heap to efficiently track top-k results without
    /// storing all chunks in memory. Also enforces a max scan limit to
    /// prevent performance issues with very large datasets.
    pub async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        project_filter: Option<&[String]>,
        min_score: Option<f32>,
    ) -> Result<Vec<SearchResult>, String> {
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        // Maximum chunks to scan (prevents runaway queries on large datasets)
        const MAX_SCAN_LIMIT: usize = 10000;

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

        // Use a min-heap to efficiently track top-k results
        // We wrap in Reverse to make it a min-heap (lowest score at top)
        #[derive(PartialEq)]
        struct ScoredResult {
            score: f32,
            result: SearchResult,
        }

        impl Eq for ScoredResult {}

        impl PartialOrd for ScoredResult {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                // Reverse ordering for min-heap behavior
                other.score.partial_cmp(&self.score)
            }
        }

        impl Ord for ScoredResult {
            fn cmp(&self, other: &Self) -> Ordering {
                self.partial_cmp(other).unwrap_or(Ordering::Equal)
            }
        }

        let mut heap: BinaryHeap<ScoredResult> = BinaryHeap::with_capacity(top_k + 1);
        let threshold = min_score.unwrap_or(0.0);
        let mut scanned = 0;

        while let Some(row) = rows.next().map_err(|e| format!("Row error: {}", e))? {
            scanned += 1;

            // Enforce scan limit
            if scanned > MAX_SCAN_LIMIT {
                warn!(
                    "Vector search hit scan limit ({}) - results may be incomplete. Consider filtering by project.",
                    MAX_SCAN_LIMIT
                );
                break;
            }

            let embedding_blob: Vec<u8> = row.get(6).map_err(|e| format!("Get embedding: {}", e))?;

            // Convert bytes back to f32 vec
            let embedding: Vec<f32> = embedding_blob
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            let score = cosine_similarity(query_embedding, &embedding);

            // Skip if below threshold
            if score < threshold {
                continue;
            }

            // Skip if heap is full and this score is worse than the minimum in heap
            if heap.len() >= top_k {
                if let Some(min) = heap.peek() {
                    if score <= min.score {
                        continue;
                    }
                }
            }

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

            heap.push(ScoredResult { score, result });

            // If we have more than top_k, remove the lowest
            if heap.len() > top_k {
                heap.pop();
            }
        }

        debug!("Vector search scanned {} chunks, found {} results", scanned, heap.len());

        // Extract results and sort by score descending
        let mut results: Vec<SearchResult> = heap.into_iter().map(|sr| sr.result).collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        Ok(results)
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

    // ========================================================================
    // Incremental Indexing Support
    // ========================================================================

    /// Get all indexed files for a project with their modification times
    /// Returns HashMap<file_path, mtime>
    pub async fn get_indexed_files(
        &self,
        project_path: &str,
    ) -> Result<std::collections::HashMap<String, i64>, String> {
        let conn = self.conn.lock().await;

        let mut stmt = conn
            .prepare("SELECT file_path, mtime FROM indexed_files WHERE project_path = ?1")
            .map_err(|e| format!("Prepare failed: {}", e))?;

        let rows = stmt
            .query_map(params![project_path], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("Query failed: {}", e))?;

        let mut files = std::collections::HashMap::new();
        for row in rows {
            let (path, mtime) = row.map_err(|e| format!("Row error: {}", e))?;
            files.insert(path, mtime);
        }

        debug!(
            "Retrieved {} indexed files for project {}",
            files.len(),
            project_path
        );
        Ok(files)
    }

    /// Track an indexed file with its modification time
    pub async fn track_indexed_file(
        &self,
        project_path: &str,
        file_path: &str,
        mtime: i64,
        chunk_count: i32,
    ) -> Result<(), String> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR REPLACE INTO indexed_files (project_path, file_path, mtime, indexed_at, chunk_count)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                project_path,
                file_path,
                mtime,
                chrono::Utc::now().to_rfc3339(),
                chunk_count
            ],
        )
        .map_err(|e| format!("Track file failed: {}", e))?;

        debug!("Tracked indexed file: {} (mtime: {})", file_path, mtime);
        Ok(())
    }

    /// Delete all data for a project atomically (chunks + tracking in single transaction)
    pub async fn delete_project_complete(&self, project_path: &str) -> Result<usize, String> {
        let conn = self.conn.lock().await;

        conn.execute("BEGIN TRANSACTION", [])
            .map_err(|e| format!("Begin transaction failed: {}", e))?;

        let result = (|| {
            let deleted = conn
                .execute(
                    "DELETE FROM document_chunks WHERE project_path = ?1",
                    params![project_path],
                )
                .map_err(|e| format!("Delete chunks failed: {}", e))?;

            conn.execute(
                "DELETE FROM indexed_files WHERE project_path = ?1",
                params![project_path],
            )
            .map_err(|e| format!("Delete tracking failed: {}", e))?;

            Ok::<usize, String>(deleted)
        })();

        match result {
            Ok(deleted) => {
                conn.execute("COMMIT", [])
                    .map_err(|e| format!("Commit failed: {}", e))?;
                info!(
                    "Deleted {} chunks and all tracking for project {} (atomic)",
                    deleted, project_path
                );
                Ok(deleted)
            }
            Err(e) => {
                conn.execute("ROLLBACK", []).ok();
                Err(e)
            }
        }
    }

    /// Delete file data atomically (chunks + tracking in single transaction)
    pub async fn delete_file_complete(
        &self,
        project_path: &str,
        file_path: &str,
    ) -> Result<usize, String> {
        let conn = self.conn.lock().await;

        conn.execute("BEGIN TRANSACTION", [])
            .map_err(|e| format!("Begin transaction failed: {}", e))?;

        let result = (|| {
            let deleted = conn
                .execute(
                    "DELETE FROM document_chunks WHERE project_path = ?1 AND file_path = ?2",
                    params![project_path, file_path],
                )
                .map_err(|e| format!("Delete chunks failed: {}", e))?;

            conn.execute(
                "DELETE FROM indexed_files WHERE project_path = ?1 AND file_path = ?2",
                params![project_path, file_path],
            )
            .map_err(|e| format!("Delete tracking failed: {}", e))?;

            Ok::<usize, String>(deleted)
        })();

        match result {
            Ok(deleted) => {
                conn.execute("COMMIT", [])
                    .map_err(|e| format!("Commit failed: {}", e))?;
                debug!(
                    "Deleted {} chunks and tracking for file {} (atomic)",
                    deleted, file_path
                );
                Ok(deleted)
            }
            Err(e) => {
                conn.execute("ROLLBACK", []).ok();
                Err(e)
            }
        }
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
        // Use id as chunk_index to ensure uniqueness (UNIQUE constraint on project_path, file_path, chunk_index)
        let chunk_index: i32 = id.parse().unwrap_or(0);
        StoredChunk {
            id: id.to_string(),
            project_path: "/test/project".to_string(),
            file_path: "test.md".to_string(),
            chunk_index,
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
    async fn test_delete_project_complete() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = VectorStore::new(db_path).unwrap();

        let chunk = create_test_chunk("1", "Test content", vec![1.0, 0.0, 0.0]);
        store.upsert_chunks(vec![chunk]).await.unwrap();

        let statuses = store.get_status(None).await.unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].total_chunks, 1);

        store.delete_project_complete("/test/project").await.unwrap();

        let statuses = store.get_status(None).await.unwrap();
        assert_eq!(statuses.len(), 0);
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
