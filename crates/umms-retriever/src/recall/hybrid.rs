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
    #[instrument(skip(self), fields(query, agent = %agent_id))]
    pub async fn recall(
        &self,
        query: &str,
        agent_id: &AgentId,
        query_vector: &[f32],
    ) -> Result<Vec<ScoredMemory>> {
        let top_k = self.config.top_k_recall;
        let bm25_weight = self.config.bm25_weight;
        let vector_weight = 1.0 - bm25_weight;

        // 1. BM25 sparse search
        let bm25_results = self.bm25.search(query, agent_id, top_k, true)?;

        // 2. Vector ANN search
        let vector_results = self.vector
            .search(agent_id, query_vector, top_k, true)
            .await?;

        // 3. Reciprocal Rank Fusion
        //
        // RRF score = sum_over_lists( weight / (k + rank) )
        // where k=60 is a constant that prevents top-ranked items from
        // dominating too heavily.
        const RRF_K: f32 = 60.0;

        // Map: memory_id → (rrf_score, Option<ScoredMemory>)
        let mut fused: HashMap<String, (f32, Option<ScoredMemory>)> = HashMap::new();

        // BM25 contributions
        for (rank, (id, _bm25_score)) in bm25_results.iter().enumerate() {
            let rrf = bm25_weight / (RRF_K + rank as f32 + 1.0);
            fused
                .entry(id.clone())
                .and_modify(|(score, _)| *score += rrf)
                .or_insert((rrf, None));
        }

        // Vector contributions
        for (rank, sm) in vector_results.iter().enumerate() {
            let rrf = vector_weight / (RRF_K + rank as f32 + 1.0);
            let id = sm.entry.id.as_str().to_owned();
            fused
                .entry(id)
                .and_modify(|(score, entry)| {
                    *score += rrf;
                    if entry.is_none() {
                        *entry = Some(sm.clone());
                    }
                })
                .or_insert((rrf, Some(sm.clone())));
        }

        // 4. Sort by fused score descending, take top_k
        let mut ranked: Vec<(String, f32, Option<ScoredMemory>)> = fused
            .into_iter()
            .map(|(id, (score, sm))| (id, score, sm))
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(top_k);

        // 5. Build final results — for entries that came from BM25 only,
        //    we need to fetch the full MemoryEntry from the vector store.
        let mut results = Vec::with_capacity(ranked.len());
        for (id, score, maybe_sm) in ranked {
            if let Some(mut sm) = maybe_sm {
                sm.score = score;
                sm.source = ScoreSource::Hybrid;
                results.push(sm);
            } else {
                // Entry found by BM25 but not by vector search —
                // fetch from store. If it's gone, skip silently.
                let Ok(mid) = umms_core::types::MemoryId::from_str(&id) else {
                    continue;
                };
                if let Ok(Some(entry)) = self.vector.get(&mid).await {
                    results.push(ScoredMemory {
                        entry,
                        score,
                        source: ScoreSource::Hybrid,
                    });
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    // Hybrid recall tests require both a BM25 index and a vector store,
    // so they live in integration tests rather than unit tests.
}
