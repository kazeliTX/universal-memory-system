//! Three-stage retrieval pipeline: recall → rerank → diffusion.
//!
//! Implements the `Retriever` trait from `umms-core`. Auto-escalates
//! search depth per ADR-012: if recall returns fewer results than
//! `escalation_threshold`, the pipeline extends into graph diffusion.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::instrument;

use umms_core::config::RetrieverConfig;
use umms_core::error::Result;
use umms_core::traits::{
    Encoder, KnowledgeGraphStore, Retriever, RetrievalLatency, RetrievalResult, VectorStore,
};
use umms_core::types::{AgentId, ScoredMemory, ScoreSource};

use crate::recall::bm25::Bm25Index;
use crate::recall::hybrid::HybridRecall;

/// The full retrieval pipeline.
pub struct RetrievalPipeline {
    hybrid: HybridRecall,
    encoder: Arc<dyn Encoder>,
    graph: Arc<dyn KnowledgeGraphStore>,
    config: RetrieverConfig,
}

impl RetrievalPipeline {
    pub fn new(
        bm25: Arc<Bm25Index>,
        vector: Arc<dyn VectorStore>,
        encoder: Arc<dyn Encoder>,
        graph: Arc<dyn KnowledgeGraphStore>,
        config: RetrieverConfig,
    ) -> Self {
        let hybrid = HybridRecall::new(
            bm25,
            Arc::clone(&vector),
            Arc::clone(&encoder),
            config.clone(),
        );
        Self { hybrid, encoder, graph, config }
    }

    /// Stage 2: Rerank using cosine similarity re-scoring.
    ///
    /// For now, use simple cosine re-scoring against the query vector.
    /// Future: cross-encoder or LLM-based reranking.
    fn rerank(
        &self,
        candidates: Vec<ScoredMemory>,
        query_vector: &[f32],
    ) -> Vec<ScoredMemory> {
        let top_k = self.config.top_k_rerank;
        let mut scored: Vec<ScoredMemory> = candidates
            .into_iter()
            .map(|mut sm| {
                // Re-score using cosine similarity if vector is available
                if let Some(ref vec) = sm.entry.vector {
                    if !vec.is_empty() {
                        sm.score = cosine_similarity(query_vector, vec);
                    }
                }
                sm
            })
            .collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    /// Stage 3: LIF cognitive diffusion — expand results via knowledge graph.
    ///
    /// For each reranked result, find related nodes in the graph up to
    /// `lif_hops` away. Entries connected to those nodes are added as
    /// diffusion-discovered results.
    #[instrument(skip(self, reranked), fields(agent = %agent_id, hops = self.config.lif_hops))]
    async fn diffuse(
        &self,
        reranked: &[ScoredMemory],
        agent_id: &AgentId,
    ) -> Result<Vec<ScoredMemory>> {
        if self.config.lif_hops == 0 {
            return Ok(Vec::new());
        }

        let mut diffusion_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let existing_ids: std::collections::HashSet<String> = reranked
            .iter()
            .map(|sm| sm.entry.id.as_str().to_owned())
            .collect();

        // For each reranked entry, find graph nodes with matching labels
        // and traverse their neighborhoods.
        for sm in reranked.iter().take(5) {
            // Use first few words as label query
            let label_query = sm
                .entry
                .content_text
                .as_deref()
                .unwrap_or("")
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ");

            if label_query.is_empty() {
                continue;
            }

            let nodes = self
                .graph
                .find_nodes(&label_query, Some(agent_id), 3)
                .await?;

            for node in &nodes {
                let (traversed_nodes, _edges) = self
                    .graph
                    .traverse(&node.id, self.config.lif_hops, Some(agent_id))
                    .await?;

                for tn in &traversed_nodes {
                    // Node labels that differ from the original query
                    // represent diffusion-discovered concepts.
                    if !diffusion_ids.contains(&tn.label)
                        && diffusion_ids.len() < self.config.lif_max_nodes
                    {
                        diffusion_ids.insert(tn.id.as_str().to_owned());
                    }
                }
            }
        }

        // For now, diffusion just returns node info; future versions will
        // look up associated memory entries in the vector store.
        // This is a placeholder for the full LIF implementation.
        Ok(Vec::new())
    }
}

#[async_trait]
impl Retriever for RetrievalPipeline {
    #[instrument(skip(self), fields(query, agent = %agent_id))]
    async fn retrieve(
        &self,
        query: &str,
        agent_id: &AgentId,
    ) -> Result<RetrievalResult> {
        let total_start = std::time::Instant::now();
        let mut latency = RetrievalLatency::default();

        // Stage 0: Encode query
        let encode_start = std::time::Instant::now();
        let query_vector = self.encoder.encode_text(query).await?;
        latency.encode_ms = encode_start.elapsed().as_millis() as u64;

        // Stage 1: Hybrid recall
        let recall_start = std::time::Instant::now();
        let recalled = self.hybrid.recall(query, agent_id, &query_vector).await?;
        latency.recall_ms = recall_start.elapsed().as_millis() as u64;

        // Stage 2: Rerank
        let rerank_start = std::time::Instant::now();
        let reranked = self.rerank(recalled, &query_vector);
        latency.rerank_ms = rerank_start.elapsed().as_millis() as u64;

        // Stage 3: Diffusion (auto-escalate per ADR-012)
        let diffusion_entries = if self.config.auto_escalate
            && reranked.len() < self.config.escalation_threshold
        {
            let diff_start = std::time::Instant::now();
            let diff = self.diffuse(&reranked, agent_id).await?;
            latency.diffusion_ms = diff_start.elapsed().as_millis() as u64;
            diff
        } else if self.config.lif_hops > 0 {
            // Always run diffusion if configured, even when results are sufficient
            let diff_start = std::time::Instant::now();
            let diff = self.diffuse(&reranked, agent_id).await?;
            latency.diffusion_ms = diff_start.elapsed().as_millis() as u64;
            diff
        } else {
            Vec::new()
        };

        // Final: take top_k_final
        let mut final_results = reranked;
        final_results.truncate(self.config.top_k_final);

        // Filter by min_score
        if self.config.min_score > 0.0 {
            final_results.retain(|sm| sm.score >= self.config.min_score);
        }

        latency.total_ms = total_start.elapsed().as_millis() as u64;

        Ok(RetrievalResult {
            entries: final_results,
            diffusion_entries,
            latency,
        })
    }

    async fn recall_only(
        &self,
        query: &str,
        agent_id: &AgentId,
        top_k: usize,
    ) -> Result<Vec<ScoredMemory>> {
        let query_vector = self.encoder.encode_text(query).await?;
        let mut results = self.hybrid.recall(query, agent_id, &query_vector).await?;
        results.truncate(top_k);
        Ok(results)
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
