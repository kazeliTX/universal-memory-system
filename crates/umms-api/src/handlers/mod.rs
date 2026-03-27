//! HTTP handler modules, organised by domain.
//!
//! Each module maps 1:1 to a section of the API surface. Handlers are pure
//! functions: extract state + params → compute → return response type.
//! No business logic lives here — that belongs in the storage/service crates.

pub mod agent;
pub mod audit;
pub mod benchmarks;
pub mod consolidation;
pub mod encoder;
pub mod epa;
pub mod files;
pub mod graph;
pub mod ingest;
pub mod memory;
pub mod system;
pub mod tags;
