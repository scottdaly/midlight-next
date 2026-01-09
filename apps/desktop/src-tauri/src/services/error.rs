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

/// Import-specific errors
#[derive(Error, Debug, Clone)]
#[allow(dead_code)] // Some variants reserved for future error handling
pub enum ImportError {
    #[error("Invalid filename: {0}")]
    InvalidFilename(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Path traversal detected: {0}")]
    PathTraversal(String),

    #[error("File too large: {0}")]
    FileTooLarge(String),

    #[error("YAML parsing error: {0}")]
    YamlParse(String),

    #[error("CSV parsing error: {0}")]
    CsvParse(String),

    #[error("Dangerous URL scheme: {0}")]
    DangerousScheme(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Insufficient disk space")]
    InsufficientDiskSpace,

    #[error("Import cancelled")]
    Cancelled,

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("{0}")]
    Other(String),
}

impl From<std::io::Error> for ImportError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::PermissionDenied => ImportError::PermissionDenied(err.to_string()),
            std::io::ErrorKind::NotFound => ImportError::FileNotFound(err.to_string()),
            _ => ImportError::Io(err.to_string()),
        }
    }
}

impl serde::Serialize for ImportError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, MidlightError>;
