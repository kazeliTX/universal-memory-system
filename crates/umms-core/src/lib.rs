
//! # umms-core
//!
//! Core types, traits, and error definitions for the UMMS memory system.
//! This crate contains no business logic — only shared data structures
//! and trait interfaces that other crates depend on.
//!
//! ## Module organization
//! - `ids` — Newtype ID wrappers (AgentId, MemoryId, SessionId, ...)
//! - `enums` — Domain enums (Modality, MemoryLayer, DecayCategory, ...)
//! - `memory` — MemoryEntry, query types, knowledge graph types
//! - `traits` — Storage and service trait contracts
//! - `error` — Unified error hierarchy
//! - `types` — Convenience re-export of ids + enums + memory

pub mod config;
pub mod enums;
pub mod error;
pub mod ids;
pub mod memory;
pub mod traits;
pub mod types;

// Top-level re-exports for convenience
pub use error::{Result, StorageError, UmmsError};
pub use traits::*;
pub use types::*;
