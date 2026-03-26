//! Document ingestion pipeline: parse → chunk → contextualize → encode → store.
//!
//! Supports the "one LLM call per document" strategy (ADR / DocSkeleton):
//! 1. Extract document skeleton (title, summary, sections) — 1 LLM call
//! 2. Split into chunks by semantic boundaries
//! 3. Inject skeleton context into each chunk (pure string concat, 0 API calls)
//! 4. Batch-encode contextualized chunks
//! 5. Store in VectorStore + index in BM25

pub mod chunker;
pub mod pipeline;
pub mod skeleton;
pub mod tag_extractor;
