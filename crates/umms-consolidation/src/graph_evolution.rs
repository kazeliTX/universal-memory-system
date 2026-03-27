//! Graph Evolution — detect and merge similar knowledge graph nodes.
//!
//! Over time, the knowledge graph accumulates duplicate or near-duplicate
//! nodes (e.g., "Rust" and "Rust lang" referring to the same concept).
//! Graph evolution scans for similar node pairs and merges them,
//! consolidating edges and properties.
//!
//! It also strengthens edges between nodes that are frequently accessed
//! together, reinforcing well-trodden pathways in the knowledge graph.

use std::time::Instant;

use serde::Serialize;
use tracing::{debug, info, instrument, warn};

use umms_core::error::Result;
use umms_core::traits::KnowledgeGraphStore;
use umms_core::types::{AgentId, KgNode};

/// Result of a graph evolution pass.
#[derive(Debug, Clone, Serialize)]
pub struct EvolutionResult {
    /// Number of candidate pairs scanned for similarity.
    pub pairs_scanned: usize,
    /// Number of node pairs that were merged.
    pub nodes_merged: usize,
    /// Number of edges whose weights were strengthened.
    pub edges_strengthened: usize,
    /// Wall-clock time in milliseconds.
    pub elapsed_ms: u64,
}

/// Configuration and logic for knowledge graph evolution.
pub struct GraphEvolution {
    /// Minimum similarity score (0.0..=1.0) for two nodes to be merge candidates.
    min_similarity: f32,
    /// Maximum number of merges per evolution run (to limit blast radius).
    max_merge_per_run: usize,
}

impl GraphEvolution {
    /// Create a new graph evolution engine with the given parameters.
    pub fn new(min_similarity: f32, max_merge_per_run: usize) -> Self {
        Self {
            min_similarity,
            max_merge_per_run,
        }
    }

    /// Create with sensible defaults (similarity >= 0.8, max 10 merges per run).
    pub fn with_defaults() -> Self {
        Self::new(0.8, 10)
    }

    /// Merge properties from two nodes, preferring the surviving node's values
    /// but incorporating the absorbed node's properties when the surviving node
    /// lacks them.
    fn merge_properties(surviving: &KgNode, absorbed: &KgNode) -> serde_json::Value {
        let mut merged = match &surviving.properties {
            serde_json::Value::Object(map) => serde_json::Value::Object(map.clone()),
            _ => serde_json::json!({}),
        };

        if let serde_json::Value::Object(absorbed_map) = &absorbed.properties {
            if let serde_json::Value::Object(ref mut merged_map) = merged {
                for (key, value) in absorbed_map {
                    // Only insert if surviving doesn't already have this key.
                    merged_map.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
        }

        // Track merge provenance.
        if let serde_json::Value::Object(ref mut map) = merged {
            map.insert(
                "_merged_from".to_string(),
                serde_json::json!(absorbed.id.as_str()),
            );
        }

        merged
    }

    /// Scan for similar node pairs and merge them.
    ///
    /// Uses [`KnowledgeGraphStore::find_similar_node_pairs`] to discover
    /// candidates, then merges pairs above the similarity threshold.
    /// The node with the higher importance score survives; the other
    /// is absorbed.
    #[instrument(skip(self, graph), fields(agent_id = ?agent_id))]
    pub async fn evolve(
        &self,
        graph: &dyn KnowledgeGraphStore,
        agent_id: Option<&AgentId>,
    ) -> Result<EvolutionResult> {
        let start = Instant::now();

        let pairs = graph
            .find_similar_node_pairs(agent_id, self.min_similarity, self.max_merge_per_run * 2)
            .await?;

        let pairs_scanned = pairs.len();
        info!(pairs_scanned, "Found candidate pairs for merge");

        let mut nodes_merged: usize = 0;

        for (node_a, node_b, similarity) in &pairs {
            if nodes_merged >= self.max_merge_per_run {
                debug!(
                    max = self.max_merge_per_run,
                    "Reached merge limit, stopping"
                );
                break;
            }

            // The node with higher importance survives.
            let (surviving, absorbed) = if node_a.importance >= node_b.importance {
                (node_a, node_b)
            } else {
                (node_b, node_a)
            };

            debug!(
                surviving = %surviving.id,
                absorbed = %absorbed.id,
                similarity,
                "Merging nodes"
            );

            let merged_props = Self::merge_properties(surviving, absorbed);

            match graph
                .merge_nodes(&surviving.id, &absorbed.id, merged_props)
                .await
            {
                Ok(redirected_edges) => {
                    debug!(
                        surviving = %surviving.id,
                        absorbed = %absorbed.id,
                        redirected = redirected_edges.len(),
                        "Merge complete"
                    );
                    nodes_merged += 1;
                }
                Err(e) => {
                    warn!(
                        surviving = %surviving.id,
                        absorbed = %absorbed.id,
                        error = %e,
                        "Failed to merge nodes, skipping pair"
                    );
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        info!(pairs_scanned, nodes_merged, elapsed_ms, "Evolution complete");

        Ok(EvolutionResult {
            pairs_scanned,
            nodes_merged,
            edges_strengthened: 0,
            elapsed_ms,
        })
    }

    /// Strengthen edges between frequently co-accessed nodes.
    ///
    /// Scans all nodes for the agent, finds edges whose endpoint nodes
    /// both have high access importance, and increases those edge weights.
    /// This reinforces well-trodden pathways in the knowledge graph.
    #[instrument(skip(self, graph), fields(agent_id = ?agent_id))]
    pub async fn strengthen_edges(
        &self,
        graph: &dyn KnowledgeGraphStore,
        agent_id: Option<&AgentId>,
    ) -> Result<usize> {
        // Get all nodes for the agent to build an importance lookup.
        let agent_id_owned;
        let nodes = match agent_id {
            Some(aid) => {
                agent_id_owned = aid.clone();
                graph.nodes_for_agent(&agent_id_owned, true).await?
            }
            None => {
                // Without an agent_id, we cannot call nodes_for_agent.
                // Return 0 strengthened edges.
                debug!("No agent_id provided, skipping edge strengthening");
                return Ok(0);
            }
        };

        let node_importance: std::collections::HashMap<String, f32> = nodes
            .iter()
            .map(|n| (n.id.as_str().to_owned(), n.importance))
            .collect();

        let mut updates: Vec<(umms_core::types::EdgeId, f32)> = Vec::new();

        for node in &nodes {
            let edges = graph.edges_of(&node.id).await?;
            for edge in &edges {
                // Only process each edge once (from source side).
                if edge.source_id != node.id {
                    continue;
                }

                let source_imp = node_importance
                    .get(edge.source_id.as_str())
                    .copied()
                    .unwrap_or(0.0);
                let target_imp = node_importance
                    .get(edge.target_id.as_str())
                    .copied()
                    .unwrap_or(0.0);

                // Both endpoints must have above-average importance.
                let avg_imp = (source_imp + target_imp) / 2.0;
                if avg_imp > 0.5 {
                    // Boost weight by 10%, capped at 1.0.
                    let new_weight = (edge.weight * 1.1).min(1.0);
                    if (new_weight - edge.weight).abs() > f32::EPSILON {
                        updates.push((edge.id.clone(), new_weight));
                    }
                }
            }
        }

        let count = updates.len();
        if !updates.is_empty() {
            graph.batch_update_edge_weights(&updates).await?;
        }

        info!(edges_strengthened = count, "Edge strengthening complete");
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_properties_prefers_surviving() {
        let surviving = KgNode {
            id: umms_core::types::NodeId::new(),
            agent_id: None,
            node_type: umms_core::types::KgNodeType::Entity,
            label: "Rust".into(),
            properties: serde_json::json!({"lang": "rust", "year": 2015}),
            importance: 0.9,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let absorbed = KgNode {
            id: umms_core::types::NodeId::new(),
            agent_id: None,
            node_type: umms_core::types::KgNodeType::Entity,
            label: "Rust lang".into(),
            properties: serde_json::json!({"lang": "rust-lang", "creator": "Graydon Hoare"}),
            importance: 0.5,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let merged = GraphEvolution::merge_properties(&surviving, &absorbed);
        let map = merged.as_object().unwrap();

        // Surviving's value wins for "lang".
        assert_eq!(map.get("lang").unwrap(), "rust");
        // Absorbed's unique key "creator" is included.
        assert_eq!(map.get("creator").unwrap(), "Graydon Hoare");
        // Surviving's "year" is preserved.
        assert_eq!(map.get("year").unwrap(), 2015);
        // Provenance is tracked.
        assert!(map.contains_key("_merged_from"));
    }

    #[test]
    fn merge_properties_handles_null_properties() {
        let surviving = KgNode {
            id: umms_core::types::NodeId::new(),
            agent_id: None,
            node_type: umms_core::types::KgNodeType::Concept,
            label: "A".into(),
            properties: serde_json::Value::Null,
            importance: 0.5,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let absorbed = KgNode {
            id: umms_core::types::NodeId::new(),
            agent_id: None,
            node_type: umms_core::types::KgNodeType::Concept,
            label: "B".into(),
            properties: serde_json::json!({"key": "value"}),
            importance: 0.3,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let merged = GraphEvolution::merge_properties(&surviving, &absorbed);
        let map = merged.as_object().unwrap();
        assert_eq!(map.get("key").unwrap(), "value");
    }

    #[test]
    fn default_parameters() {
        let ge = GraphEvolution::with_defaults();
        assert!((ge.min_similarity - 0.8).abs() < f32::EPSILON);
        assert_eq!(ge.max_merge_per_run, 10);
    }
}
