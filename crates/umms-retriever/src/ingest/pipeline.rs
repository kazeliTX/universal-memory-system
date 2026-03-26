//! Document ingestion pipeline: the full flow from raw text to stored memories.
//!
//! Flow: text → chunk → skeleton extract (1 LLM call) → contextualize (0 calls)
//!       → batch encode → store in VectorStore + BM25

use std::sync::Arc;

use tracing::{info, instrument, warn};

use umms_core::error::Result;
use umms_core::traits::{Encoder, VectorStore};
use umms_core::types::{AgentId, IsolationScope, MemoryEntryBuilder, MemoryLayer, Modality};

use crate::recall::Bm25Index;

use super::chunker::{ChunkerConfig, chunk_text};
use super::skeleton::DocSkeleton;

/// Result of ingesting a document.
#[derive(Debug)]
pub struct IngestResult {
    /// Number of chunks created.
    pub chunks_created: usize,
    /// Number of chunks successfully stored.
    pub chunks_stored: usize,
    /// Document skeleton (if extraction succeeded).
    pub skeleton: DocSkeleton,
    /// Total ingestion time in milliseconds.
    pub total_ms: u64,
    /// Per-stage latency.
    pub latency: IngestLatency,
}

/// Per-stage latency for the ingestion pipeline.
#[derive(Debug, Default)]
pub struct IngestLatency {
    pub chunk_ms: u64,
    pub skeleton_ms: u64,
    pub encode_ms: u64,
    pub store_ms: u64,
}

/// The document ingestion pipeline.
pub struct IngestPipeline {
    encoder: Arc<dyn Encoder>,
    vector_store: Arc<dyn VectorStore>,
    bm25: Arc<Bm25Index>,
    chunker_config: ChunkerConfig,
}

impl IngestPipeline {
    pub fn new(
        encoder: Arc<dyn Encoder>,
        vector_store: Arc<dyn VectorStore>,
        bm25: Arc<Bm25Index>,
        chunker_config: ChunkerConfig,
    ) -> Self {
        Self {
            encoder,
            vector_store,
            bm25,
            chunker_config,
        }
    }

    /// Ingest a document: chunk → contextualize → encode → store.
    ///
    /// `skeleton` is optional — if `None`, a fallback skeleton is generated
    /// from the document text (no LLM call). Pass a pre-extracted skeleton
    /// if you've already called the LLM for extraction.
    #[instrument(skip(self, text, skeleton), fields(agent = %agent_id, text_len = text.len()))]
    pub async fn ingest(
        &self,
        text: &str,
        agent_id: &AgentId,
        scope: IsolationScope,
        tags: Vec<String>,
        skeleton: Option<DocSkeleton>,
    ) -> Result<IngestResult> {
        let total_start = std::time::Instant::now();
        let mut latency = IngestLatency::default();

        // Stage 1: Chunk
        let chunk_start = std::time::Instant::now();
        let chunks = chunk_text(text, &self.chunker_config);
        latency.chunk_ms = chunk_start.elapsed().as_millis() as u64;

        if chunks.is_empty() {
            info!("Empty document, nothing to ingest");
            return Ok(IngestResult {
                chunks_created: 0,
                chunks_stored: 0,
                skeleton: DocSkeleton::fallback(text, 0),
                total_ms: total_start.elapsed().as_millis() as u64,
                latency,
            });
        }

        info!(chunks = chunks.len(), "Document chunked");

        // Stage 2: Skeleton (use provided or generate fallback)
        let skel_start = std::time::Instant::now();
        let skel = skeleton.unwrap_or_else(|| DocSkeleton::fallback(text, chunks.len()));
        latency.skeleton_ms = skel_start.elapsed().as_millis() as u64;

        // Stage 3: Contextualize + Encode
        // Build contextualized texts for embedding
        let contextualized: Vec<String> = chunks
            .iter()
            .map(|c| skel.contextualize(c.index, &c.text))
            .collect();

        let encode_start = std::time::Instant::now();
        let vectors = self.encoder.encode_batch(&contextualized).await?;
        latency.encode_ms = encode_start.elapsed().as_millis() as u64;

        if vectors.len() != chunks.len() {
            warn!(
                expected = chunks.len(),
                got = vectors.len(),
                "Vector count mismatch after encoding"
            );
        }

        // Stage 4: Build MemoryEntries and store
        let store_start = std::time::Instant::now();
        let mut entries = Vec::with_capacity(chunks.len());

        for (i, (chunk, vector)) in chunks.iter().zip(vectors.into_iter()).enumerate() {
            let mut chunk_tags = tags.clone();
            chunk_tags.push(format!("chunk:{i}"));
            chunk_tags.push(format!("doc:{}", skel.title));

            if let Some(section) = skel.section_for(i) {
                chunk_tags.push(format!("section:{}", section.title));
            }

            let entry = MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
                .layer(MemoryLayer::EpisodicMemory)
                .scope(scope.clone())
                .content_text(&chunk.text)
                .vector(vector)
                .importance(0.5)
                .tags(chunk_tags)
                .build();

            entries.push(entry);
        }

        // Batch insert into vector store
        self.vector_store.insert_batch(&entries).await?;

        // Index in BM25
        self.bm25.index_batch(&entries).await?;

        latency.store_ms = store_start.elapsed().as_millis() as u64;

        let stored = entries.len();
        info!(
            chunks_created = chunks.len(),
            chunks_stored = stored,
            total_ms = total_start.elapsed().as_millis() as u64,
            "Document ingestion complete"
        );

        Ok(IngestResult {
            chunks_created: chunks.len(),
            chunks_stored: stored,
            skeleton: skel,
            total_ms: total_start.elapsed().as_millis() as u64,
            latency,
        })
    }
}
