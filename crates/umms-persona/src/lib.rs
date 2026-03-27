
//! # umms-persona
//!
//! Persona templates and configuration-driven memory behavior profiles.
//!
//! This crate provides:
//! - [`AgentPersona`] — data model for agent identity and expertise
//! - [`PersonaStore`] — SQLite-backed CRUD storage for personas
//! - [`AgentMatcher`] — content-to-agent matching based on expertise keywords
//! - [`defaults`] — pre-defined persona templates seeded on first run

pub mod defaults;
pub mod matcher;
pub mod persona;
pub mod store;

pub use defaults::default_personas;
pub use matcher::AgentMatcher;
pub use persona::{AgentPersona, AgentRetrievalConfig};
pub use store::PersonaStore;
