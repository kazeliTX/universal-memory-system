//! Graph node creation for ingested document chunks.
//!
//! When chunks are stored during ingestion, `GraphBuilder` creates a KgNode
//! for each chunk and connects consecutive chunks with "follows" edges and
//! chunks sharing tags with "shares_tag" edges. This links the vector store
//! (L2) to the knowledge graph (L3) so that LIF graph diffusion can discover
//! related memories during retrieval.

use std::collections::HashMap;

use tracing::{info, warn};

use umms_core::error::Result;
use umms_core::traits::KnowledgeGraphStore;
use umms_core::types::{AgentId, KgEdge, KgNode, KgNodeType, NodeId};

/// Builds knowledge graph structure from ingested document chunks.
pub struct GraphBuilder;

impl GraphBuilder {
    /// Create graph nodes and edges for a batch of ingested chunks.
    ///
    /// - One `KgNode` per chunk, with `label` set to the memory_id for exact
    ///   lookup via `find_nodes`.
    /// - Consecutive chunks are connected with "follows" edges (weight 1.0).
    /// - Chunks sharing tags are connected with "shares_tag" edges (weight 0.5).
    ///
    /// Returns `(nodes_created, edges_created)`. Failures are logged as warnings
    /// and do not abort the entire operation.
    pub async fn build_from_chunks(
        graph: &dyn KnowledgeGraphStore,
        memory_ids: &[String],
        texts: &[String],
        agent_id: &AgentId,
        tags_per_chunk: &[Vec<String>],
    ) -> Result<(usize, usize)> {
        if memory_ids.is_empty() {
            return Ok((0, 0));
        }

        let mut nodes_created: usize = 0;
        let mut edges_created: usize = 0;
        let mut node_ids: Vec<NodeId> = Vec::with_capacity(memory_ids.len());

        // --- Create one KgNode per chunk ---
        for (i, mid) in memory_ids.iter().enumerate() {
            let preview = texts
                .get(i)
                .map(|t| {
                    let chars: String = t.chars().take(100).collect();
                    chars
                })
                .unwrap_or_default();

            let node = KgNode {
                id: NodeId::new(),
                agent_id: Some(agent_id.clone()),
                node_type: KgNodeType::Entity,
                label: mid.clone(),
                properties: serde_json::json!({
                    "memory_id": mid,
                    "preview": preview,
                }),
                importance: 0.5,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            match graph.add_node(&node).await {
                Ok(nid) => {
                    node_ids.push(nid);
                    nodes_created += 1;
                }
                Err(e) => {
                    warn!(memory_id = %mid, error = %e, "Failed to create graph node for chunk");
                    // Push a placeholder so indices stay aligned
                    node_ids.push(node.id.clone());
                }
            }
        }

        // --- Connect consecutive chunks with "follows" edges ---
        for i in 0..node_ids.len().saturating_sub(1) {
            let edge = KgEdge {
                id: Default::default(),
                source_id: node_ids[i].clone(),
                target_id: node_ids[i + 1].clone(),
                relation: "follows".to_owned(),
                weight: 1.0,
                agent_id: Some(agent_id.clone()),
                created_at: chrono::Utc::now(),
            };

            match graph.add_edge(&edge).await {
                Ok(_) => edges_created += 1,
                Err(e) => {
                    warn!(src = i, dst = i + 1, error = %e, "Failed to create 'follows' edge");
                }
            }
        }

        // --- Connect chunks sharing tags with "shares_tag" edges ---
        // Build tag -> chunk indices map
        let mut tag_to_chunks: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, chunk_tags) in tags_per_chunk.iter().enumerate() {
            for tag in chunk_tags {
                tag_to_chunks.entry(tag.as_str()).or_default().push(i);
            }
        }

        // For each tag shared by multiple chunks, connect pairs
        for (_tag, chunk_indices) in &tag_to_chunks {
            if chunk_indices.len() < 2 {
                continue;
            }
            // Limit to avoid quadratic blowup on very common tags
            let limit = chunk_indices.len().min(10);
            for a in 0..limit {
                for b in (a + 1)..limit {
                    let idx_a = chunk_indices[a];
                    let idx_b = chunk_indices[b];
                    // Skip consecutive pairs (already connected by "follows")
                    if idx_b == idx_a + 1 {
                        continue;
                    }

                    let edge = KgEdge {
                        id: Default::default(),
                        source_id: node_ids[idx_a].clone(),
                        target_id: node_ids[idx_b].clone(),
                        relation: "shares_tag".to_owned(),
                        weight: 0.5,
                        agent_id: Some(agent_id.clone()),
                        created_at: chrono::Utc::now(),
                    };

                    match graph.add_edge(&edge).await {
                        Ok(_) => edges_created += 1,
                        Err(e) => {
                            warn!(
                                chunk_a = idx_a,
                                chunk_b = idx_b,
                                error = %e,
                                "Failed to create 'shares_tag' edge"
                            );
                        }
                    }
                }
            }
        }

        info!(
            nodes = nodes_created,
            edges = edges_created,
            "Graph nodes and edges created for ingested chunks"
        );

        Ok((nodes_created, edges_created))
    }
}
