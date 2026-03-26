//! Re-exports all core types from their focused modules.
//!
//! Downstream crates can still `use umms_core::types::*` and get everything.
//! Internally, types are organized by domain concern:
//! - `ids.rs`: Newtype ID wrappers (AgentId, MemoryId, ...)
//! - `enums.rs`: Domain enums (Modality, MemoryLayer, ...)
//! - `memory.rs`: MemoryEntry, query types, knowledge graph types

pub use std::str::FromStr;

pub use crate::enums::*;
pub use crate::ids::*;
pub use crate::memory::*;
pub use crate::tag::*;
