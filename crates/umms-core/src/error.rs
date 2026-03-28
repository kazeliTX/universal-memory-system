//! Unified error types for the UMMS system.
//!
//! Design principle: errors carry diagnostic context so you can understand
//! what went wrong without a debugger. Every error variant includes enough
//! information to answer "what were we trying to do, and why did it fail?"

use thiserror::Error;

use crate::types::{AgentId, MemoryId};

/// Top-level error type for the UMMS system.
#[derive(Error, Debug)]
pub enum UmmsError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Encoding(#[from] EncodingError),

    #[error(transparent)]
    Retrieval(#[from] RetrievalError),

    #[error(transparent)]
    Persona(#[from] PersonaError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, UmmsError>;

// ---------------------------------------------------------------------------
// Storage errors — carry the operation context
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Failed to connect to {backend}: {reason}")]
    ConnectionFailed { backend: String, reason: String },

    #[error("Failed to write memory {memory_id} for agent {agent_id}: {reason}")]
    WriteFailed {
        memory_id: MemoryId,
        agent_id: AgentId,
        reason: String,
    },

    #[error("Failed to read memory {memory_id}: {reason}")]
    ReadFailed { memory_id: MemoryId, reason: String },

    #[error("Memory entry not found: {0}")]
    NotFound(MemoryId),

    #[error("Agent not found: {0}")]
    AgentNotFound(AgentId),

    #[error("Snapshot failed for agent {agent_id}: {reason}")]
    SnapshotFailed { agent_id: AgentId, reason: String },

    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    #[error("SQLite error: {0}")]
    Sqlite(String),

    #[error("LanceDB error: {0}")]
    Lance(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Encoding errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("API call to {provider} failed: {reason}")]
    ApiCallFailed { provider: String, reason: String },

    #[error("API timeout after {timeout_ms}ms from {provider}")]
    ApiTimeout { provider: String, timeout_ms: u64 },

    #[error("Local model not available at {path}: {reason}")]
    LocalModelUnavailable { path: String, reason: String },

    #[error("Unsupported modality for encoding: {0:?}")]
    UnsupportedModality(crate::types::Modality),
}

// ---------------------------------------------------------------------------
// Retrieval errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum RetrievalError {
    #[error("Search index not ready for {backend}: {reason}")]
    IndexNotReady { backend: String, reason: String },

    #[error("Query encoding failed: {0}")]
    QueryEncodingFailed(String),

    #[error("Search failed: {0}")]
    SearchFailed(String),
}

// ---------------------------------------------------------------------------
// Persona errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum PersonaError {
    #[error("Agent config invalid for '{agent_id}': {reason}")]
    InvalidConfig { agent_id: String, reason: String },

    #[error("Persona template not found: {0}")]
    TemplateNotFound(String),

    #[error("Template render failed: {0}")]
    RenderFailed(String),

    #[error("Access denied: agent '{agent_id}' cannot access '{resource}'")]
    AccessDenied { agent_id: String, resource: String },
}
