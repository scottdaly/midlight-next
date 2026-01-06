// Error types for Midlight services

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MidlightError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Workspace not initialized: {0}")]
    WorkspaceNotInitialized(String),

    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(String),

    #[error("Object not found: {0}")]
    ObjectNotFound(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, MidlightError>;
