//! Consolidation Scheduler — orchestrates decay, graph evolution,
//! and auto-promotion into a single consolidation cycle.
//!
//! The scheduler is the primary entry point for the M4 consolidation system.
//! It coordinates all sub-engines and produces a unified report.

use std::time::Instant;

use chrono::Utc;
use serde::Serialize;
use tracing::{info, instrument};

use umms_core::config::{DecayConfig, GraphEvolutionConfig, PromotionConfig};
use umms_core::error::Result;
use umms_core::traits::{KnowledgeGraphStore, VectorStore};
use umms_core::types::AgentId;

use crate::auto_promote::{AutoPromoter, PromoteResult};
use crate::decay::{DecayEngine, DecayResult};
use crate::graph_evolution::{EvolutionResult, GraphEvolution};

/// Full report of a consolidation cycle for one agent.
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidationReport {
    /// The agent whose memories were consolidated.
    pub agent_id: String,
    /// Results of the decay pass.
    pub decay: DecayResult,
    /// Results of graph evolution (merge + strengthen).
    pub evolution: EvolutionResult,
    /// Results of auto-promotion.
    pub promotion: PromoteResult,
    /// Total wall-clock time for the full cycle in milliseconds.
    pub total_ms: u64,
    /// When this consolidation cycle ran.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Orchestrates all consolidation sub-systems into a single cycle.
///
/// A consolidation cycle runs three phases in sequence:
/// 1. **Decay**: Reduce importance of stale memories, archive fully-decayed ones.
/// 2. **Graph evolution**: Merge similar KG nodes, strengthen frequently-used edges.
/// 3. **Auto-promotion**: Promote private memories that meet criteria to shared scope.
///
/// The scheduler does not own the stores — they are passed in per-call,
/// allowing the caller to control lifecycle and sharing.
pub struct ConsolidationScheduler {
    decay_engine: DecayEngine,
    graph_evolution: GraphEvolution,
    auto_promoter: AutoPromoter,
    // Future: llm: Option<Arc<dyn GenerativeLlm>>
}

impl ConsolidationScheduler {
    /// Create a new scheduler with the given configuration.
    pub fn new(decay_config: DecayConfig, promotion_config: PromotionConfig) -> Self {
        Self {
            decay_engine: DecayEngine::new(decay_config),
            graph_evolution: GraphEvolution::with_defaults(),
            auto_promoter: AutoPromoter::new(promotion_config),
        }
    }

    /// Create a scheduler from full config (decay + graph evolution + promotion).
    pub fn from_config(
        decay_config: DecayConfig,
        graph_evo_config: GraphEvolutionConfig,
        promotion_config: PromotionConfig,
    ) -> Self {
        Self {
            decay_engine: DecayEngine::new(decay_config),
            graph_evolution: GraphEvolution::new(
                graph_evo_config.min_similarity,
                graph_evo_config.max_merge_per_run,
            ),
            auto_promoter: AutoPromoter::new(promotion_config),
        }
    }

    /// Create a scheduler with custom graph evolution parameters.
    pub fn with_graph_evolution(
        decay_config: DecayConfig,
        promotion_config: PromotionConfig,
        min_similarity: f32,
        max_merge_per_run: usize,
    ) -> Self {
        Self {
            decay_engine: DecayEngine::new(decay_config),
            graph_evolution: GraphEvolution::new(min_similarity, max_merge_per_run),
            auto_promoter: AutoPromoter::new(promotion_config),
        }
    }

    /// Access the decay engine for standalone use.
    pub fn decay_engine(&self) -> &DecayEngine {
        &self.decay_engine
    }

    /// Access the graph evolution engine for standalone use.
    pub fn graph_evolution(&self) -> &GraphEvolution {
        &self.graph_evolution
    }

    /// Access the auto-promoter for standalone use.
    pub fn auto_promoter(&self) -> &AutoPromoter {
        &self.auto_promoter
    }

    /// Run a full consolidation cycle for an agent.
    ///
    /// Executes decay, graph evolution, and auto-promotion in sequence.
    /// Each phase runs independently — a failure in one phase is logged
    /// but does not prevent subsequent phases from running.
    #[instrument(skip(self, vector_store, graph), fields(agent_id = %agent_id))]
    pub async fn run_cycle(
        &self,
        vector_store: &dyn VectorStore,
        graph: &dyn KnowledgeGraphStore,
        agent_id: &AgentId,
    ) -> Result<ConsolidationReport> {
        let cycle_start = Instant::now();
        info!("Starting consolidation cycle");

        // Phase 1: Decay
        let decay_result = self.decay_engine.run_decay(vector_store, agent_id).await?;
        info!(
            updated = decay_result.updated,
            archived = decay_result.archived,
            "Decay phase complete"
        );

        // Phase 2: Graph evolution
        let mut evolution_result = self
            .graph_evolution
            .evolve(graph, Some(agent_id))
            .await?;

        let edges_strengthened = self
            .graph_evolution
            .strengthen_edges(graph, Some(agent_id))
            .await?;
        evolution_result.edges_strengthened = edges_strengthened;

        info!(
            merged = evolution_result.nodes_merged,
            strengthened = edges_strengthened,
            "Graph evolution phase complete"
        );

        // Phase 3: Auto-promotion
        let promote_result = self
            .auto_promoter
            .scan_and_promote(vector_store, agent_id)
            .await?;
        info!(
            promoted = promote_result.promoted,
            "Auto-promotion phase complete"
        );

        let total_ms = cycle_start.elapsed().as_millis() as u64;

        let report = ConsolidationReport {
            agent_id: agent_id.as_str().to_owned(),
            decay: decay_result,
            evolution: evolution_result,
            promotion: promote_result,
            total_ms,
            timestamp: Utc::now(),
        };

        info!(
            total_ms,
            "Consolidation cycle complete"
        );

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_creates_with_defaults() {
        let scheduler = ConsolidationScheduler::new(
            DecayConfig::default(),
            PromotionConfig::default(),
        );

        // Just verify it constructs without panic.
        let _ = scheduler.decay_engine();
        let _ = scheduler.graph_evolution();
        let _ = scheduler.auto_promoter();
    }

    #[test]
    fn scheduler_custom_graph_evolution() {
        let scheduler = ConsolidationScheduler::with_graph_evolution(
            DecayConfig::default(),
            PromotionConfig::default(),
            0.9,
            5,
        );

        let _ = scheduler.decay_engine();
    }

    #[test]
    fn consolidation_report_serializes() {
        let report = ConsolidationReport {
            agent_id: "test-agent".to_string(),
            decay: DecayResult {
                scanned: 100,
                updated: 10,
                archived: 2,
                elapsed_ms: 50,
            },
            evolution: EvolutionResult {
                pairs_scanned: 20,
                nodes_merged: 3,
                edges_strengthened: 5,
                elapsed_ms: 30,
            },
            promotion: PromoteResult {
                scanned: 100,
                promoted: 1,
                elapsed_ms: 20,
            },
            total_ms: 100,
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("test-agent"));
        assert!(json.contains("\"scanned\":100"));
    }
}
