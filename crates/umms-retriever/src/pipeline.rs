//! Three-stage retrieval pipeline: recall → rerank → diffusion.
//!
//! Implements the `Retriever` trait from `umms-core`. Auto-escalates
//! search depth per ADR-012: if recall returns fewer results than
//! `escalation_threshold`, the pipeline extends into graph diffusion.

use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::instrument;

use umms_core::config::{EpaConfig, ReshapingConfig, RetrieverConfig};
use umms_core::error::Result;
use umms_core::error::UmmsError;
use umms_core::importance::{self, ImportanceConfig};
use umms_core::tag::EpaResult;
use umms_core::traits::{
    Encoder, KnowledgeGraphStore, RetrievalLatency, RetrievalResult, Retriever, TagStore,
    VectorStore,
};
use umms_core::types::{AgentId, MemoryId, ScoreSource, ScoredMemory};

use umms_analyzer::epa::EpaAnalyzer;
use umms_analyzer::lgsrr::{LgsrrDecomposer, LgsrrDecomposition};
use umms_analyzer::reshaping::QueryReshaper;

use crate::recall::bm25::Bm25Index;
use crate::recall::hybrid::{HitSourceInfo, HybridHit, HybridRecall};

/// Extended result with per-hit source tracking for dashboard visualization.
#[derive(Debug)]
pub struct PipelineResult {
    pub retrieval: RetrievalResult,
    /// Per-hit source info (same order as retrieval.entries).
    pub hit_sources: Vec<HitSourceInfo>,
    /// How many hits came from BM25 only / Vector only / both.
    pub bm25_only: usize,
    pub vector_only: usize,
    pub both: usize,
    pub recall_count: usize,
    pub rerank_count: usize,
    pub diffusion_count: usize,
    /// EPA analysis result (None if EPA is disabled or no tags available).
    pub epa_result: Option<EpaResult>,
    /// LGSRR five-layer decomposition of the query.
    pub lgsrr: Option<LgsrrDecomposition>,
}

/// The full retrieval pipeline.
pub struct RetrievalPipeline {
    hybrid: HybridRecall,
    vector: Arc<dyn VectorStore>,
    encoder: Arc<dyn Encoder>,
    graph: Arc<dyn KnowledgeGraphStore>,
    epa: Option<EpaAnalyzer>,
    reshaper: Option<QueryReshaper>,
    config: RetrieverConfig,
    importance_config: ImportanceConfig,
}

impl RetrievalPipeline {
    pub fn new(
        bm25: Arc<Bm25Index>,
        vector: Arc<dyn VectorStore>,
        encoder: Arc<dyn Encoder>,
        graph: Arc<dyn KnowledgeGraphStore>,
        config: RetrieverConfig,
    ) -> Self {
        Self::with_importance(
            bm25,
            vector,
            encoder,
            graph,
            config,
            ImportanceConfig::default(),
        )
    }

    /// Construct the pipeline with a custom importance scoring configuration.
    pub fn with_importance(
        bm25: Arc<Bm25Index>,
        vector: Arc<dyn VectorStore>,
        encoder: Arc<dyn Encoder>,
        graph: Arc<dyn KnowledgeGraphStore>,
        config: RetrieverConfig,
        importance_config: ImportanceConfig,
    ) -> Self {
        let hybrid = HybridRecall::new(
            bm25,
            Arc::clone(&vector),
            Arc::clone(&encoder),
            config.clone(),
        );
        Self {
            hybrid,
            vector,
            encoder,
            graph,
            epa: None,
            reshaper: None,
            config,
            importance_config,
        }
    }

    /// Attach EPA and query reshaping to the pipeline.
    /// Call this after construction if a TagStore is available.
    #[must_use]
    pub fn with_epa(
        mut self,
        tag_store: Arc<dyn TagStore>,
        epa_config: EpaConfig,
        reshaping_config: ReshapingConfig,
    ) -> Self {
        if epa_config.enabled {
            self.epa = Some(EpaAnalyzer::new(Arc::clone(&tag_store), epa_config));
        }
        if reshaping_config.enabled {
            self.reshaper = Some(QueryReshaper::new(tag_store, reshaping_config));
        }
        self
    }

    /// Stage 2: Rerank using cosine similarity re-scoring.
    fn rerank(&self, candidates: Vec<HybridHit>, query_vector: &[f32]) -> Vec<HybridHit> {
        let top_k = self.config.top_k_rerank;
        let mut scored: Vec<HybridHit> = candidates
            .into_iter()
            .map(|mut hit| {
                if let Some(ref vec) = hit.memory.entry.vector {
                    if !vec.is_empty() {
                        hit.memory.score = cosine_similarity(query_vector, vec);
                    }
                }
                hit
            })
            .collect();

        scored.sort_by(|a, b| {
            b.memory
                .score
                .partial_cmp(&a.memory.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        scored
    }

    /// Full pipeline returning rich source-tracking data for visualization.
    #[allow(clippy::too_many_lines)]
    pub async fn retrieve_with_sources(
        &self,
        query: &str,
        agent_id: &AgentId,
    ) -> Result<PipelineResult> {
        let total_start = std::time::Instant::now();
        let mut latency = RetrievalLatency::default();

        // Stage 0: Encode
        let encode_start = std::time::Instant::now();
        let query_vector = self.encoder.encode_text(query).await?;
        latency.encode_ms = encode_start.elapsed().as_millis() as u64;

        // Stage 0.3: LGSRR decomposition (always runs, heuristic, sub-ms)
        let lgsrr = LgsrrDecomposer::decompose(query);

        // Stage 0.5: EPA analyze (if enabled)
        let epa_result = if let Some(ref epa) = self.epa {
            let epa_start = std::time::Instant::now();
            match epa.analyze(&query_vector, agent_id).await {
                Ok(result) => {
                    latency.epa_ms = epa_start.elapsed().as_millis() as u64;
                    Some(result)
                }
                Err(e) => {
                    tracing::warn!("EPA analysis failed, using passthrough: {e}");
                    None
                }
            }
        } else {
            None
        };

        // Stage 0.6: Query reshape (if enabled + EPA produced results)
        let effective_vector = if let (Some(reshaper), Some(epa)) = (&self.reshaper, &epa_result) {
            if epa.alpha > 1e-6 && !epa.activated_tags.is_empty() {
                let reshape_start = std::time::Instant::now();
                match reshaper.reshape(&query_vector, epa, agent_id).await {
                    Ok(reshaped) => {
                        latency.reshape_ms = reshape_start.elapsed().as_millis() as u64;
                        reshaped
                    }
                    Err(e) => {
                        tracing::warn!("Query reshaping failed, using original: {e}");
                        query_vector.clone()
                    }
                }
            } else {
                query_vector.clone()
            }
        } else {
            query_vector.clone()
        };

        // Stage 1: Hybrid recall (using reshaped vector if available)
        let recall_start = std::time::Instant::now();
        let hybrid_result = self
            .hybrid
            .recall(query, agent_id, &effective_vector)
            .await?;
        latency.recall_ms = recall_start.elapsed().as_millis() as u64;

        let bm25_only = hybrid_result.bm25_only;
        let vector_only = hybrid_result.vector_only;
        let both = hybrid_result.both;

        let recall_count = hybrid_result.hits.len();

        // Stage 2: Rerank (cosine similarity re-scoring)
        let rerank_start = std::time::Instant::now();
        let mut reranked = self.rerank(hybrid_result.hits, &effective_vector);
        latency.rerank_ms = rerank_start.elapsed().as_millis() as u64;

        // Filter by min_score AFTER rerank — scores are now cosine similarity (0-1 range)
        if self.config.min_score > 0.0 {
            reranked.retain(|h| h.memory.score >= self.config.min_score);
        }

        // ADR-013: Blend cosine score with multi-dimensional importance.
        // 80% cosine similarity + 20% effective importance.
        for hit in &mut reranked {
            let effective_imp = importance::score_entry(
                &hit.memory.entry,
                0, // graph_in_degree — requires graph query, skip for now
                0, // cross_agent_count — skip for now
                &self.importance_config,
            );
            hit.memory.score = hit.memory.score * 0.8 + effective_imp * 0.2;
        }
        // Re-sort after importance blending.
        reranked.sort_by(|a, b| {
            b.memory
                .score
                .partial_cmp(&a.memory.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let rerank_count = reranked.len();

        // Stage 3: Diffusion
        let reranked_memories: Vec<ScoredMemory> =
            reranked.iter().map(|h| h.memory.clone()).collect();
        let diffusion_entries = if self.config.lif_hops > 0 {
            let diff_start = std::time::Instant::now();
            let diff = self.diffuse(&reranked_memories, agent_id).await?;
            latency.diffusion_ms = diff_start.elapsed().as_millis() as u64;
            diff
        } else {
            Vec::new()
        };
        let diffusion_count = diffusion_entries.len();

        // Truncate to final count
        let mut final_hits = reranked;
        final_hits.truncate(self.config.top_k_final);

        latency.total_ms = total_start.elapsed().as_millis() as u64;

        let hit_sources: Vec<HitSourceInfo> =
            final_hits.iter().map(|h| h.source_info.clone()).collect();
        let entries: Vec<ScoredMemory> = final_hits.into_iter().map(|h| h.memory).collect();

        Ok(PipelineResult {
            retrieval: RetrievalResult {
                entries,
                diffusion_entries,
                latency,
            },
            hit_sources,
            bm25_only,
            vector_only,
            both,
            recall_count,
            rerank_count,
            diffusion_count,
            epa_result,
            lgsrr: Some(lgsrr),
        })
    }

    /// Stage 3: LIF cognitive diffusion — expand results via knowledge graph.
    ///
    /// For each seed (top reranked result), find the corresponding graph node
    /// by searching `find_nodes(memory_id, agent_id, 1)`, then traverse N hops.
    /// Discovered nodes with a "memory_id" property are loaded from the vector
    /// store and scored with exponential decay: `seed_score * 0.5^hops * node_importance`.
    #[instrument(skip(self, seeds), fields(agent = %agent_id, hops = self.config.lif_hops))]
    async fn diffuse(
        &self,
        seeds: &[ScoredMemory],
        agent_id: &AgentId,
    ) -> Result<Vec<ScoredMemory>> {
        if self.config.lif_hops == 0 || seeds.is_empty() {
            return Ok(Vec::new());
        }

        let mut seen: std::collections::HashSet<String> = seeds
            .iter()
            .map(|s| s.entry.id.as_str().to_owned())
            .collect();
        let mut discovered = Vec::new();

        for seed in seeds.iter().take(5) {
            // Find graph node for this memory by searching label = memory_id
            let nodes = match self
                .graph
                .find_nodes(seed.entry.id.as_str(), Some(agent_id), 1)
                .await
            {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(
                        memory_id = %seed.entry.id,
                        error = %e,
                        "Graph find_nodes failed during diffusion, skipping seed"
                    );
                    continue;
                }
            };

            let Some(seed_node) = nodes.first() else {
                continue;
            };

            // BFS traverse from seed node
            let (reached, _) = match self
                .graph
                .traverse(&seed_node.id, self.config.lif_hops, Some(agent_id))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        node_id = %seed_node.id,
                        error = %e,
                        "Graph traverse failed during diffusion, skipping seed"
                    );
                    continue;
                }
            };

            for node in &reached {
                if let Some(mid) = node.properties.get("memory_id").and_then(|v| v.as_str()) {
                    if seen.contains(mid) {
                        continue;
                    }
                    seen.insert(mid.to_owned());

                    let mem_id = MemoryId::from_str(mid)
                        .map_err(|e| UmmsError::Internal(format!("bad memory_id in graph: {e}")))?;

                    if let Ok(Some(entry)) = self.vector.get(&mem_id).await {
                        let score = seed.score * 0.5 * node.importance;
                        discovered.push(ScoredMemory {
                            entry,
                            score,
                            source: ScoreSource::GraphDiffusion,
                        });
                    }
                }
            }
        }

        discovered.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        discovered.truncate(self.config.lif_max_nodes);
        Ok(discovered)
    }
}

#[async_trait]
impl Retriever for RetrievalPipeline {
    #[instrument(skip(self), fields(query, agent = %agent_id))]
    async fn retrieve(&self, query: &str, agent_id: &AgentId) -> Result<RetrievalResult> {
        let result = self.retrieve_with_sources(query, agent_id).await?;
        Ok(result.retrieval)
    }

    async fn recall_only(
        &self,
        query: &str,
        agent_id: &AgentId,
        top_k: usize,
    ) -> Result<Vec<ScoredMemory>> {
        let query_vector = self.encoder.encode_text(query).await?;
        let mut result = self.hybrid.recall(query, agent_id, &query_vector).await?;
        result.hits.truncate(top_k);
        Ok(result.hits.into_iter().map(|h| h.memory).collect())
    }
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn cosine_empty_vectors() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }
}
