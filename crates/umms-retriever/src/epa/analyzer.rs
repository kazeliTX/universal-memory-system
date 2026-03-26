//! EpaAnalyzer — the main EPA orchestrator.
//!
//! Coordinates tag activation, K-Means clustering, PCA decomposition,
//! and metric computation to produce an `EpaResult` for each query.

use std::sync::Arc;

use umms_core::config::{EpaConfig, RetrieverConfig};
use umms_core::error::Result;
use umms_core::tag::{ActivatedTag, DynamicRetrieverParams, EpaResult};
use umms_core::traits::TagStore;
use umms_core::types::AgentId;

use super::kmeans::weighted_kmeans;
use super::pca::power_iteration_pca;

/// Embedding Projection Analysis engine.
///
/// Holds a reference to the tag store and EPA configuration. Designed to be
/// shared across async tasks via `Arc<EpaAnalyzer>`.
pub struct EpaAnalyzer {
    tag_store: Arc<dyn TagStore>,
    config: EpaConfig,
}

impl EpaAnalyzer {
    pub fn new(tag_store: Arc<dyn TagStore>, config: EpaConfig) -> Self {
        Self { tag_store, config }
    }

    /// Analyze a query vector against the tag embedding space.
    ///
    /// Steps:
    /// 1. Search tag store for nearest tags (activation).
    /// 2. Filter by activation threshold.
    /// 3. Cluster activated tag embeddings with weighted K-Means.
    /// 4. Compute logic_depth and cross_domain_resonance from cluster stats.
    /// 5. Extract semantic axes via power iteration PCA.
    /// 6. Compute alpha blending factor.
    pub async fn analyze(
        &self,
        query_vector: &[f32],
        agent_id: &AgentId,
    ) -> Result<EpaResult> {
        // Step 1: Search for nearest tags
        let matches = self
            .tag_store
            .search_by_vector(query_vector, Some(agent_id), self.config.activation_top_k)
            .await?;

        // Step 2: Filter by activation threshold
        let activated: Vec<_> = matches
            .into_iter()
            .filter(|m| m.similarity >= self.config.activation_threshold)
            .collect();

        if activated.is_empty() {
            return Ok(EpaResult::passthrough());
        }

        // Build activated tag list for the result
        let activated_tags: Vec<ActivatedTag> = activated
            .iter()
            .map(|m| ActivatedTag {
                tag_id: m.tag.id.clone(),
                label: m.tag.label.clone(),
                similarity: m.similarity,
            })
            .collect();

        // Collect embeddings and weights for clustering / PCA
        let embeddings: Vec<&[f32]> = activated
            .iter()
            .map(|m| m.tag.vector.as_slice())
            .collect();
        let weights: Vec<f32> = activated.iter().map(|m| m.similarity).collect();

        // Step 3: Weighted K-Means clustering
        let k = self.config.num_clusters.min(activated.len());
        let clusters = weighted_kmeans(
            &embeddings,
            &weights,
            k,
            self.config.kmeans_iterations,
        );

        // Step 4: Compute metrics
        let total_weight: f32 = clusters.iter().map(|c| c.total_weight).sum();
        let max_cluster_weight = clusters
            .iter()
            .map(|c| c.total_weight)
            .fold(0.0_f32, f32::max);

        // logic_depth: how focused the query is (dominant cluster / total)
        let logic_depth = if total_weight > 0.0 {
            (max_cluster_weight / total_weight).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // cross_domain_resonance: fraction of clusters that are "significant"
        let significant_count = clusters
            .iter()
            .filter(|c| {
                total_weight > 0.0
                    && (c.total_weight / total_weight)
                        >= self.config.cluster_significance_threshold
            })
            .count();
        let cross_domain_resonance = if k > 0 {
            (significant_count as f32 / k as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Step 5: PCA semantic axes
        let semantic_axes = power_iteration_pca(
            &embeddings,
            &weights,
            self.config.num_axes,
            self.config.pca_iterations,
        );

        // Step 6: Compute alpha
        // Average importance of activated tags
        let avg_importance = if !activated.is_empty() {
            activated.iter().map(|m| m.tag.importance).sum::<f32>()
                / activated.len() as f32
        } else {
            0.0
        };

        let alpha = (self.config.alpha_base
            + self.config.alpha_depth_weight * logic_depth
            + self.config.alpha_resonance_weight * cross_domain_resonance
            + self.config.alpha_importance_weight * avg_importance)
            .clamp(self.config.alpha_min, self.config.alpha_max);

        Ok(EpaResult {
            logic_depth,
            cross_domain_resonance,
            semantic_axes,
            activated_tags,
            alpha,
        })
    }

    /// Compute dynamic retriever parameters from an EPA result.
    ///
    /// Policy:
    /// - High logic_depth (focused query) → reduce recall breadth, increase precision
    /// - High cross_domain_resonance (multi-topic) → increase recall breadth, more diffusion hops
    pub fn dynamic_params(
        &self,
        epa: &EpaResult,
        base: &RetrieverConfig,
    ) -> DynamicRetrieverParams {
        let depth = epa.logic_depth;
        let resonance = epa.cross_domain_resonance;

        // BM25 weight: focused queries benefit more from semantic search,
        // so reduce BM25 weight when depth is high.
        let bm25_weight =
            (base.bm25_weight * (1.0 - 0.3 * depth) * (1.0 + 0.2 * resonance))
                .clamp(0.05, 0.8);

        // Recall breadth: focused → fewer candidates suffice;
        // resonant → need more to cover multiple domains.
        let recall_scale = 1.0 - 0.3 * depth + 0.3 * resonance;
        let top_k_recall = ((base.top_k_recall as f32 * recall_scale) as usize)
            .max(20)
            .min(base.top_k_recall * 2);

        // Rerank: tighter for focused queries
        let rerank_scale = 1.0 - 0.2 * depth + 0.2 * resonance;
        let top_k_rerank = ((base.top_k_rerank as f32 * rerank_scale) as usize)
            .max(5)
            .min(base.top_k_rerank * 2);

        // Min score: higher bar when focused (we want precision)
        let min_score = (base.min_score + 0.05 * depth).clamp(0.0, 0.5);

        // LIF hops: more when cross-domain (explore connections between topics)
        let lif_hops = if resonance > 0.5 {
            (base.lif_hops + 1).min(5)
        } else if depth > 0.7 {
            base.lif_hops.saturating_sub(1).max(1)
        } else {
            base.lif_hops
        };

        DynamicRetrieverParams {
            bm25_weight,
            top_k_recall,
            top_k_rerank,
            min_score,
            lif_hops,
        }
    }
}
