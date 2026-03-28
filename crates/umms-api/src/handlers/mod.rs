//! HTTP handler modules, organised by domain.
//!
//! Each module maps 1:1 to a section of the API surface. Handlers are pure
//! functions: extract state + params → compute → return response type.
//! No business logic lives here — that belongs in the storage/service crates.

pub mod agent;
pub mod analyze;
pub mod audit;
pub mod benchmarks;
pub mod chat;
pub mod consolidation;
pub mod diary;
pub mod encoder;
pub mod epa;
pub mod files;
pub mod graph;
pub mod ingest;
pub mod memory;
pub mod models;
pub mod prompts;
pub mod sessions;
pub mod system;
pub mod tags;
pub mod traces;
pub mod ws;
