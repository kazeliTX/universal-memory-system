//! Unified error types for the UMMS system.

use thiserror::Error;

/// Top-level error type for the UMMS system.
#[derive(Error, Debug)]
pub enum UmmsError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Encoding error: {0}")]
    Encoding(#[from] EncodingError),

    #[error("Retrieval error: {0}")]
    Retrieval(#[from] RetrievalError),

    #[error("Persona error: {0}")]
    Persona(#[from] PersonaError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Write failed: {0}")]
    WriteFailed(String),

    #[error("Read failed: {0}")]
    ReadFailed(String),

    #[error("Entry not found: {0}")]
    NotFound(String),

    #[error("Snapshot failed: {0}")]
    SnapshotFailed(String),

    #[error("Migration failed: {0}")]
    MigrationFailed(String),
}

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("API call failed: {0}")]
    ApiCallFailed(String),

    #[error("API timeout after {0}ms")]
    ApiTimeout(u64),

    #[error("Local model not available: {0}")]
    LocalModelUnavailable(String),

    #[error("Unsupported modality: {0}")]
    UnsupportedModality(String),
}

#[derive(Error, Debug)]
pub enum RetrievalError {
    #[error("Index not ready: {0}")]
    IndexNotReady(String),

    #[error("Query encoding failed: {0}")]
    QueryEncodingFailed(String),

    #[error("Search failed: {0}")]
    SearchFailed(String),
}

#[derive(Error, Debug)]
pub enum PersonaError {
    #[error("Agent config invalid: {0}")]
    InvalidConfig(String),

    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Render failed: {0}")]
    RenderFailed(String),

    #[error("Access denied: agent '{agent_id}' cannot access '{resource}'")]
    AccessDenied {
        agent_id: String,
        resource: String,
    },
}

/// Convenience type alias for UMMS results.
pub type Result<T> = std::result::Result<T, UmmsError>;
