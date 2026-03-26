//! Hybrid recall: fuse BM25 sparse + Vector ANN dense results.
//!
//! The fusion uses weighted Reciprocal Rank Fusion (RRF) rather than
//! raw score addition, because BM25 and cosine similarity scores live
//! on different scales and are not directly comparable.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::instrument;

use umms_core::config::RetrieverConfig;
use umms_core::error::Result;
use umms_core::traits::{Encoder, VectorStore};
use umms_core::types::{AgentId, ScoredMemory, ScoreSource};

use super::bm25::Bm25Index;

/// Per-hit source tracking for visualization.
#[derive(Debug, Clone)]
pub struct HitSourceInfo {
    pub bm25_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub bm25_contribution: f32,
    pub vector_contribution: f32,
}

/// Hybrid recall result with source tracking.
#[derive(Debug, Clone)]
pub struct HybridHit {
    pub memory: ScoredMemory,
    pub source_info: HitSourceInfo,
}

/// Hybrid recall result.
pub struct HybridResult {
    pub hits: Vec<HybridHit>,
    pub bm25_only: usize,
    pub vector_only: usize,
    pub both: usize,
}

/// Hybrid recall combining BM25 and vector search.
pub struct HybridRecall {
    bm25: Arc<Bm25Index>,
    vector: Arc<dyn VectorStore>,
    encoder: Arc<dyn Encoder>,
    config: RetrieverConfig,
}

impl HybridRecall {
    pub fn new(
        bm25: Arc<Bm25Index>,
        vector: Arc<dyn VectorStore>,
        encoder: Arc<dyn Encoder>,
        config: RetrieverConfig,
    ) -> Self {
        Self { bm25, vector, encoder, config }
    }

    /// Execute hybrid recall: BM25 ∪ Vector → RRF fusion → top-k.
    ///
    /// Returns `HybridResult` with per-hit source tracking for visualization.
    #[instrument(skip(self), fields(query, agent = %agent_id))]
    pub async fn recall(
        &self,
        query: &str,
        agent_id: &AgentId,
        query_vector: &[f32],
    ) -> Result<HybridResult> {
        let top_k = self.config.top_k_recall;
        let bm25_weight = self.config.bm25_weight;
        let vector_weight = 1.0 - bm25_weight;

        // 1. BM25 sparse search
        let bm25_results = self.bm25.search(query, agent_id, top_k, true)?;

        // 2. Vector ANN search
        let vector_results = self.vector
            .search(agent_id, query_vector, top_k, true)
            .await?;

        // 3. Reciprocal Rank Fusion with source tracking
        const RRF_K: f32 = 60.0;

        struct FusionEntry {
            rrf_score: f32,
            memory: Option<ScoredMemory>,
            bm25_rank: Option<usize>,
            vector_rank: Option<usize>,
            bm25_contribution: f32,
            vector_contribution: f32,
        }

        let mut fused: HashMap<String, FusionEntry> = HashMap::new();

        // BM25 contributions
        for (rank, (id, _bm25_score)) in bm25_results.iter().enumerate() {
            let rrf = bm25_weight / (RRF_K + rank as f32 + 1.0);
            fused
                .entry(id.clone())
                .and_modify(|e| {
                    e.rrf_score += rrf;
                    e.bm25_rank = Some(rank + 1);
                    e.bm25_contribution = rrf;
                })
                .or_insert(FusionEntry {
                    rrf_score: rrf,
                    memory: None,
                    bm25_rank: Some(rank + 1),
                    vector_rank: None,
                    bm25_contribution: rrf,
                    vector_contribution: 0.0,
                });
        }

        // Vector contributions
        for (rank, sm) in vector_results.iter().enumerate() {
            let rrf = vector_weight / (RRF_K + rank as f32 + 1.0);
            let id = sm.entry.id.as_str().to_owned();
            fused
                .entry(id)
                .and_modify(|e| {
                    e.rrf_score += rrf;
                    e.vector_rank = Some(rank + 1);
                    e.vector_contribution = rrf;
                    if e.memory.is_none() {
                        e.memory = Some(sm.clone());
                    }
                })
                .or_insert(FusionEntry {
                    rrf_score: rrf,
                    memory: Some(sm.clone()),
                    bm25_rank: None,
                    vector_rank: Some(rank + 1),
                    bm25_contribution: 0.0,
                    vector_contribution: rrf,
                });
        }

        // 4. Sort by fused score descending, take top_k
        let mut ranked: Vec<(String, FusionEntry)> = fused.into_iter().collect();
        ranked.sort_by(|a, b| {
            b.1.rrf_score
                .partial_cmp(&a.1.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(top_k);

        // 5. Count source distribution
        let mut bm25_only = 0usize;
        let mut vector_only = 0usize;
        let mut both = 0usize;

        // 6. Build final results
        let mut hits = Vec::with_capacity(ranked.len());
        for (id, entry) in ranked {
            // Track source distribution
            match (entry.bm25_rank, entry.vector_rank) {
                (Some(_), Some(_)) => both += 1,
                (Some(_), None) => bm25_only += 1,
                (None, Some(_)) => vector_only += 1,
                (None, None) => {} // shouldn't happen
            }

            let source_info = HitSourceInfo {
                bm25_rank: entry.bm25_rank,
                vector_rank: entry.vector_rank,
                bm25_contribution: entry.bm25_contribution,
                vector_contribution: entry.vector_contribution,
            };

            if let Some(mut sm) = entry.memory {
                sm.score = entry.rrf_score;
                sm.source = ScoreSource::Hybrid;
                hits.push(HybridHit { memory: sm, source_info });
            } else {
                // BM25-only hit: fetch full entry from vector store
                let Ok(mid) = umms_core::types::MemoryId::from_str(&id) else {
                    continue;
                };
                if let Ok(Some(mem)) = self.vector.get(&mid).await {
                    hits.push(HybridHit {
                        memory: ScoredMemory {
                            entry: mem,
                            score: entry.rrf_score,
                            source: ScoreSource::Hybrid,
                        },
                        source_info,
                    });
                }
            }
        }

        Ok(HybridResult { hits, bm25_only, vector_only, both })
    }
}

#[cfg(test)]
mod tests {
    // Hybrid recall tests require both a BM25 index and a vector store,
    // so they live in integration tests rather than unit tests.
}
