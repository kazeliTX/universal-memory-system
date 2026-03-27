//! Prompt Engine — VCP-inspired three-mode prompt construction system.
//!
//! Supports three editing modes:
//! - **Original**: raw text with variable substitution
//! - **Modular**: ordered blocks with variants, each independently toggleable
//! - **Preset**: load from template files (.md/.txt)
//!
//! Modules:
//! - [`types`] — data model (PromptMode, PromptBlock, AgentPromptConfig, etc.)
//! - [`engine`] — prompt builder with variable substitution
//! - [`store`] — SQLite-backed persistence for configs and warehouses
//! - [`diary_generator`] — auto-diary generation from conversation turns

pub mod diary_generator;
pub mod engine;
pub mod store;
pub mod types;

pub use engine::{PromptEngine, PromptSection, PromptTemplate};
pub use store::PromptStore;
pub use types::{
    AgentPromptConfig, BlockType, PromptBlock, PromptMode, PromptVariable, PromptWarehouse,
};
