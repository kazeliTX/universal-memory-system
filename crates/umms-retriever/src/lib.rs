//! # umms-retriever
//!
//! Three-stage memory retrieval pipeline:
//! 1. **Hybrid recall**: BM25 sparse + Vector ANN dense → RRF fusion
//! 2. **Rerank**: cosine re-scoring (future: cross-encoder)
//! 3. **LIF diffusion**: knowledge graph expansion for associative discovery
//!
//! Auto-escalates search depth per ADR-012.

pub mod ingest;
pub mod pipeline;
pub mod recall;
pub mod tokenizer;
