//! System-level handlers: health, stats, metrics, seed, clear.

use std::sync::Arc;

use axum::extract::State;
use axum::response::Json;

use umms_core::traits::{Encoder, KnowledgeGraphStore, MemoryCache, VectorStore};
use crate::response::ClearResponse;
use umms_core::types::*;
use umms_observe::{AuditEventBuilder, AuditEventType};

use crate::response::*;
use crate::state::AppState;

/// `GET /api/health`
pub async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        uptime_secs: state.started_at.elapsed().as_secs(),
        storage: StorageHealth {
            cache: "ok",
            vector: "ok",
            graph: "ok",
            files: "ok",
        },
    })
}

/// `GET /api/stats`
pub async fn stats(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    // Aggregate cache entries across all agents that have data.
    // We don't hardcode agent IDs — instead we scan graph stats and known agents.
    let known_agents = discover_agents(&state).await;

    let (mut l0, mut l1) = (0usize, 0usize);
    for agent_id in &known_agents {
        if let Ok(aid) = AgentId::from_str(agent_id) {
            for entry in state.cache.entries_for_agent(&aid).await {
                match entry.layer {
                    MemoryLayer::SensoryBuffer => l0 += 1,
                    MemoryLayer::WorkingMemory => l1 += 1,
                    _ => {}
                }
            }
        }
    }

    // Vector: total = sum of each agent's private entries + all shared entries
    let mut vector_total = 0u64;
    for agent_id in &known_agents {
        if let Ok(aid) = AgentId::from_str(agent_id) {
            vector_total += state.vector.count(&aid, false).await.unwrap_or(0);
        }
    }
    // Add shared entries once (use any agent with include_shared minus its private count)
    if let Ok(aid) = AgentId::from_str(known_agents.first().map(|s| s.as_str()).unwrap_or("_")) {
        let with_shared = state.vector.count(&aid, true).await.unwrap_or(0);
        let without = state.vector.count(&aid, false).await.unwrap_or(0);
        vector_total += with_shared.saturating_sub(without);
    }

    let gs = state.graph.stats(None).await.unwrap_or_default();

    Json(StatsResponse {
        cache: CacheStats {
            l0_entries: l0,
            l1_entries: l1,
        },
        vector: VectorStats {
            total_entries: vector_total,
        },
        graph: GraphStatsDto::from(gs),
        agents: known_agents,
    })
}

/// `GET /api/metrics` — Prometheus text format.
pub async fn metrics(State(state): State<Arc<AppState>>) -> String {
    umms_observe::encode_metrics(&state.metrics_registry)
}

/// `GET /api/demo/seed` — idempotent: clears existing demo data, then re-seeds.
pub async fn seed(State(state): State<Arc<AppState>>) -> Json<SeedResponse> {
    // Clear existing data first so repeated seeds don't accumulate
    let _ = clear_all_data(&state).await;

    let dim = state.config.vector_dim;
    let agent_names = ["coder", "researcher", "writer"];
    let sample_texts: &[&[&str]] = &[
        &[
            "Rust ownership model prevents data races at compile time",
            "Async runtime Tokio enables high-concurrency network services",
            "Pattern matching in Rust enables exhaustive case handling",
            "Cargo build system manages dependencies and compilation",
            "Trait objects provide dynamic dispatch in Rust",
        ],
        &[
            "Neural network architectures for natural language processing",
            "Transformer attention mechanism captures long-range dependencies",
            "Knowledge graph embedding methods for link prediction",
            "Retrieval augmented generation improves factual accuracy",
            "Vector databases enable semantic similarity search",
        ],
        &[
            "Technical writing requires clarity and precision",
            "API documentation should include usage examples",
            "Markdown formatting enables structured documentation",
            "Code comments explain why, not what",
            "README files should describe setup and usage",
        ],
    ];
    let mut total_memories = 0u64;

    for (idx, name) in agent_names.iter().enumerate() {
        let agent_id = match AgentId::from_str(name) {
            Ok(id) => id,
            Err(_) => continue,
        };

        let texts = sample_texts[idx];

        // Batch-encode if encoder is available, otherwise use deterministic fake vectors
        let vectors: Vec<Vec<f32>> = if let Some(enc) = &state.encoder {
            let owned: Vec<String> = texts.iter().map(|t| (*t).to_owned()).collect();
            match enc.encode_batch(&owned).await {
                Ok(vecs) => vecs,
                Err(e) => {
                    tracing::warn!("Encoder failed, falling back to fake vectors: {e}");
                    texts.iter().enumerate()
                        .map(|(i, _)| fake_vector(dim, idx, i))
                        .collect()
                }
            }
        } else {
            texts.iter().enumerate()
                .map(|(i, _)| fake_vector(dim, idx, i))
                .collect()
        };

        for (i, text) in texts.iter().enumerate() {
            let entry = MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
                .layer(MemoryLayer::EpisodicMemory)
                .scope(if i == 0 {
                    IsolationScope::Shared
                } else {
                    IsolationScope::Private
                })
                .content_text((*text).to_owned())
                .vector(vectors[i].clone())
                .importance(0.5 + (i as f32) * 0.1)
                .tags(vec!["demo".to_owned(), format!("agent-{name}")])
                .build();

            if state.vector.insert(&entry).await.is_ok() {
                // Also index in BM25 for hybrid search
                let _ = state.bm25.index_entry(&entry).await;
                state.audit.record(
                    AuditEventBuilder::new(AuditEventType::VectorInsert, name.to_string())
                        .memory_id(entry.id.as_str().to_owned())
                        .layer("L2"),
                );
                total_memories += 1;
            }
        }
    }

    // Graph nodes
    let concepts = [
        ("Rust", KgNodeType::Concept),
        ("Memory Systems", KgNodeType::Concept),
        ("LanceDB", KgNodeType::Entity),
        ("SQLite", KgNodeType::Entity),
        ("Vector Search", KgNodeType::Concept),
        ("Knowledge Graph", KgNodeType::Concept),
    ];

    let mut node_ids: Vec<NodeId> = Vec::new();
    for (label, node_type) in &concepts {
        let node = KgNode {
            id: NodeId::new(),
            agent_id: None,
            node_type: *node_type,
            label: (*label).to_owned(),
            properties: serde_json::json!({"source": "dev-seed"}),
            importance: 0.8,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        if let Ok(nid) = state.graph.add_node(&node).await {
            state.audit.record(
                AuditEventBuilder::new(AuditEventType::GraphAddNode, "_shared")
                    .node_id(nid.as_str().to_owned())
                    .layer("L3"),
            );
            node_ids.push(nid);
        }
    }

    // Graph edges
    let edge_defs = [
        (0, 1, "powers"),
        (2, 4, "enables"),
        (3, 5, "stores"),
        (1, 4, "includes"),
        (1, 5, "includes"),
    ];
    let mut edge_count = 0u64;
    for (src, tgt, rel) in &edge_defs {
        if *src < node_ids.len() && *tgt < node_ids.len() {
            let edge = KgEdge {
                id: EdgeId::new(),
                source_id: node_ids[*src].clone(),
                target_id: node_ids[*tgt].clone(),
                relation: (*rel).to_owned(),
                weight: 1.0,
                agent_id: None,
                created_at: chrono::Utc::now(),
            };
            if state.graph.add_edge(&edge).await.is_ok() {
                state.audit.record(
                    AuditEventBuilder::new(AuditEventType::GraphAddEdge, "_shared")
                        .layer("L3"),
                );
                edge_count += 1;
            }
        }
    }

    Json(SeedResponse {
        seeded: true,
        memories: total_memories,
        nodes: node_ids.len(),
        edges: edge_count,
    })
}

/// Generate a deterministic fake vector when no encoder is available.
fn fake_vector(dim: usize, agent_idx: usize, entry_idx: usize) -> Vec<f32> {
    (0..dim)
        .map(|d| ((agent_idx * 7 + entry_idx * 3 + d * 11) % 100) as f32 / 100.0)
        .collect()
}

/// Discover agent IDs that have data in the system.
///
/// This avoids hardcoding agent names. Checks vector store and graph.
async fn discover_agents(state: &AppState) -> Vec<String> {
    // For now, check a known set + any agent with graph nodes.
    // TODO(M7): replace with Persona registry query.
    let mut agents: Vec<String> = Vec::new();

    let candidate_names = ["coder", "researcher", "writer"];
    for name in &candidate_names {
        if let Ok(aid) = AgentId::from_str(name) {
            let has_vectors = state.vector.count(&aid, false).await.unwrap_or(0) > 0;
            let has_cache = state.cache.len(&aid).await > 0;
            if has_vectors || has_cache {
                agents.push(name.to_string());
            }
        }
    }

    agents
}

/// `GET /api/demo/clear` — wipe all demo data from all storage layers.
pub async fn clear(State(state): State<Arc<AppState>>) -> Json<ClearResponse> {
    let result = clear_all_data(&state).await;
    Json(result)
}

/// Shared logic for clearing all data. Used by both `clear` and `seed` (idempotent seed).
async fn clear_all_data(state: &AppState) -> ClearResponse {
    let agent_names = ["coder", "researcher", "writer"];
    let mut vectors_deleted = 0u64;
    let mut cache_evicted = 0usize;

    for name in &agent_names {
        if let Ok(aid) = AgentId::from_str(name) {
            // Evict cache
            let evicted = state.cache.evict_agent(&aid).await;
            cache_evicted += evicted.len();

            // Delete vector entries (include_shared on first agent to catch shared entries)
            let include_shared = name == &agent_names[0];
            if let Ok(count) = state.vector.delete_all(&aid, include_shared).await {
                vectors_deleted += count;
            }
        }
    }

    // Clear graph (all nodes + edges, including shared)
    let (nodes_deleted, edges_deleted) = state
        .graph
        .clear(None)
        .await
        .unwrap_or((0, 0));

    tracing::info!(
        vectors_deleted,
        nodes_deleted,
        edges_deleted,
        cache_evicted,
        "all data cleared"
    );

    ClearResponse {
        cleared: true,
        vectors_deleted,
        nodes_deleted,
        edges_deleted,
        cache_evicted,
    }
}
