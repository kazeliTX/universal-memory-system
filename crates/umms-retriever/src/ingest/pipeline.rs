//! Document ingestion pipeline: the full flow from raw text to stored memories.
//!
//! Flow: text → chunk → skeleton extract (1 LLM call) → contextualize (0 calls)
//!       → batch encode → store in VectorStore + BM25

use std::sync::Arc;

use tracing::{info, instrument, warn};

use umms_core::error::Result;
use umms_core::traits::{Encoder, KnowledgeGraphStore, VectorStore};
use umms_core::types::{AgentId, IsolationScope, MemoryEntryBuilder, MemoryLayer, Modality};
use umms_model::ModelPool;

use crate::recall::Bm25Index;

use super::chunker::{ChunkerConfig, chunk_text};
use super::graph_builder::GraphBuilder;
use super::skeleton::{self, DocSkeleton};
use super::tag_extractor::TagExtractor;

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
    /// Chunk details for visualization.
    pub chunk_details: Vec<ChunkDetail>,
    /// Number of graph nodes created for chunk linking.
    pub graph_nodes_created: usize,
    /// Number of graph edges created for chunk linking.
    pub graph_edges_created: usize,
}

/// Detail of a single chunk for dashboard visualization.
#[derive(Debug, Clone)]
pub struct ChunkDetail {
    /// Chunk index in the document.
    pub index: usize,
    /// Original chunk text (before context injection).
    pub original_text: String,
    /// Context prefix injected from skeleton.
    pub context_prefix: String,
    /// Section this chunk belongs to.
    pub section: String,
    /// Tags assigned to this chunk.
    pub tags: Vec<String>,
    /// Memory ID of the stored entry.
    pub memory_id: String,
    /// Character count of original text.
    pub char_count: usize,
}

/// Per-stage latency for the ingestion pipeline.
#[derive(Debug, Default)]
pub struct IngestLatency {
    pub chunk_ms: u64,
    pub skeleton_ms: u64,
    pub encode_ms: u64,
    pub store_ms: u64,
    pub graph_ms: u64,
}

/// The document ingestion pipeline.
pub struct IngestPipeline {
    encoder: Arc<dyn Encoder>,
    vector_store: Arc<dyn VectorStore>,
    bm25: Arc<Bm25Index>,
    chunker_config: ChunkerConfig,
    tag_extractor: Option<Arc<TagExtractor>>,
    /// Optional model pool for LLM-powered skeleton extraction.
    /// When `Some`, the pipeline uses [`skeleton::extract_skeleton_llm`] instead
    /// of the heuristic fallback.
    model_pool: Option<Arc<ModelPool>>,
    /// Optional knowledge graph store for creating chunk-level graph nodes.
    /// When `Some`, the pipeline creates KgNodes for each chunk and links them.
    graph: Option<Arc<dyn KnowledgeGraphStore>>,
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
            tag_extractor: None,
            model_pool: None,
            graph: None,
        }
    }

    /// Attach a tag extractor to the pipeline for automatic tag extraction.
    #[must_use]
    pub fn with_tag_extractor(mut self, extractor: Arc<TagExtractor>) -> Self {
        self.tag_extractor = Some(extractor);
        self
    }

    /// Attach a model pool for LLM-powered skeleton extraction.
    ///
    /// When set, [`Self::ingest`] will use the generative model to extract
    /// document structure instead of the heuristic fallback.
    #[must_use]
    pub fn with_model_pool(mut self, pool: Arc<ModelPool>) -> Self {
        self.model_pool = Some(pool);
        self
    }

    /// Attach a knowledge graph store for creating chunk-level graph nodes.
    ///
    /// When set, [`Self::ingest`] will create KgNodes for each chunk and
    /// connect them with "follows" and "shares_tag" edges.
    #[must_use]
    pub fn with_graph(mut self, graph: Arc<dyn KnowledgeGraphStore>) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Ingest a document: chunk → contextualize → encode → store.
    ///
    /// `skeleton` is optional — if `None`, a fallback skeleton is generated
    /// from the document text (no LLM call). Pass a pre-extracted skeleton
    /// if you've already called the LLM for extraction.
    #[allow(clippy::too_many_lines)]
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
                chunk_details: Vec::new(),
                graph_nodes_created: 0,
                graph_edges_created: 0,
            });
        }

        info!(chunks = chunks.len(), "Document chunked");

        // Stage 2: Skeleton (use provided, LLM, or heuristic fallback)
        let skel_start = std::time::Instant::now();
        let skel = if let Some(s) = skeleton {
            s
        } else if let Some(ref pool) = self.model_pool {
            match skeleton::extract_skeleton_llm(text, chunks.len(), pool).await {
                Ok(s) => {
                    info!(title = %s.title, entities = s.key_entities.len(), sections = s.sections.len(), "LLM skeleton extracted");
                    s
                }
                Err(e) => {
                    warn!("LLM skeleton extraction error: {e}, using fallback");
                    DocSkeleton::fallback(text, chunks.len())
                }
            }
        } else {
            DocSkeleton::fallback(text, chunks.len())
        };
        latency.skeleton_ms = skel_start.elapsed().as_millis() as u64;

        // Stage 2.5: Tag extraction (if extractor is available)
        let extracted_tags = if let Some(ref extractor) = self.tag_extractor {
            match extractor.extract(&skel, &chunks, agent_id).await {
                Ok(tag_ids) => {
                    info!(
                        chunks = tag_ids.len(),
                        total_tags = tag_ids.iter().map(Vec::len).sum::<usize>(),
                        "Tags extracted from document"
                    );
                    Some(tag_ids)
                }
                Err(e) => {
                    warn!("Tag extraction failed (continuing without tags): {e}");
                    None
                }
            }
        } else {
            None
        };

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
        let mut chunk_details = Vec::with_capacity(chunks.len());

        for (i, (chunk, vector)) in chunks.iter().zip(vectors.into_iter()).enumerate() {
            let mut chunk_tags = tags.clone();
            chunk_tags.push(format!("chunk:{i}"));
            chunk_tags.push(format!("doc:{}", skel.title));

            // Add extracted tag IDs as tag strings
            if let Some(ref tag_ids_per_chunk) = extracted_tags {
                if let Some(chunk_tag_ids) = tag_ids_per_chunk.get(i) {
                    for tag_id in chunk_tag_ids {
                        chunk_tags.push(format!("tag:{}", tag_id.as_str()));
                    }
                }
            }

            let section_name = skel
                .section_for(i)
                .map_or_else(|| "General".to_owned(), |s| s.title.clone());

            chunk_tags.push(format!("section:{section_name}"));

            let context_prefix = skel.contextualize(i, "").trim_end().to_owned();

            let entry = MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
                .layer(MemoryLayer::EpisodicMemory)
                .scope(scope)
                .content_text(&chunk.text)
                .vector(vector)
                .importance(0.5)
                .tags(chunk_tags.clone())
                .build();

            chunk_details.push(ChunkDetail {
                index: i,
                original_text: chunk.text.clone(),
                context_prefix,
                section: section_name,
                tags: chunk_tags,
                memory_id: entry.id.as_str().to_owned(),
                char_count: chunk.text.len(),
            });

            entries.push(entry);
        }

        // Batch insert into vector store
        self.vector_store.insert_batch(&entries).await?;

        // Index in BM25
        self.bm25.index_batch(&entries).await?;

        latency.store_ms = store_start.elapsed().as_millis() as u64;

        // Stage 5: Build graph nodes and edges (if graph store is available)
        let (graph_nodes_created, graph_edges_created) = if let Some(ref graph) = self.graph {
            let graph_start = std::time::Instant::now();
            let memory_ids: Vec<String> = chunk_details
                .iter()
                .map(|cd| cd.memory_id.clone())
                .collect();
            let texts: Vec<String> = chunk_details
                .iter()
                .map(|cd| cd.original_text.clone())
                .collect();
            let tags_per: Vec<Vec<String>> =
                chunk_details.iter().map(|cd| cd.tags.clone()).collect();

            let result = GraphBuilder::build_from_chunks(
                graph.as_ref(),
                &memory_ids,
                &texts,
                agent_id,
                &tags_per,
            )
            .await;

            latency.graph_ms = graph_start.elapsed().as_millis() as u64;

            match result {
                Ok((nodes, edges)) => (nodes, edges),
                Err(e) => {
                    warn!("Graph building failed (continuing without graph): {e}");
                    (0, 0)
                }
            }
        } else {
            (0, 0)
        };

        let stored = entries.len();
        info!(
            chunks_created = chunks.len(),
            chunks_stored = stored,
            graph_nodes = graph_nodes_created,
            graph_edges = graph_edges_created,
            total_ms = total_start.elapsed().as_millis() as u64,
            "Document ingestion complete"
        );

        Ok(IngestResult {
            chunks_created: chunks.len(),
            chunks_stored: stored,
            skeleton: skel,
            total_ms: total_start.elapsed().as_millis() as u64,
            latency,
            chunk_details,
            graph_nodes_created,
            graph_edges_created,
        })
    }
}
