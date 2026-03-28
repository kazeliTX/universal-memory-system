//! # umms-consolidation
//!
//! Memory consolidation through decay, graph evolution, and predictive merging.
//!
//! This crate implements the M4 Consolidation Engine, which runs periodic
//! maintenance on an agent's memory stores:
//!
//! - **Decay**: Exponential importance decay on episodic memories (L2),
//!   archiving entries that fall below threshold.
//! - **Graph evolution**: Detecting and merging similar knowledge graph nodes (L3),
//!   strengthening frequently-used edges.
//! - **Auto-promotion**: Scanning private memories and promoting qualifying
//!   entries to the shared scope.
//! - **LLM interface**: Trait definition for future generative AI capabilities
//!   (entity extraction, summarization, entity resolution).
//!
//! The [`ConsolidationScheduler`] orchestrates all sub-engines into a single
//! consolidation cycle, producing a [`ConsolidationReport`].

pub mod auto_promote;
pub mod decay;
pub mod graph_evolution;
pub mod llm;
pub mod scheduler;
pub mod wkd;

pub use auto_promote::{AutoPromoter, PromoteResult};
pub use decay::{DecayEngine, DecayResult};
pub use graph_evolution::{EvolutionResult, GraphEvolution};
pub use llm::{ExtractedEntity, ExtractedRelation, GenerativeLlm};
pub use scheduler::{ConsolidationReport, ConsolidationScheduler};
pub use wkd::{WkdEngine, WkdResult};
