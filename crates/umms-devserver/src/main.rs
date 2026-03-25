//! UMMS Dev Dashboard — a minimal diagnostic server for verifying system state.
//!
//! Start with: `cargo run -p umms-devserver`
//! Then open: <http://127.0.0.1:8720>

#![allow(clippy::missing_errors_doc)]

use std::sync::Arc;
use std::time::Instant;

use axum::{
    Router,
    extract::State,
    response::{Html, Json},
    routing::get,
};
use serde::Serialize;
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use umms_core::traits::{KnowledgeGraphStore, MemoryCache, VectorStore};
use umms_core::types::*;
use umms_storage::cache::MokaMemoryCache;
use umms_storage::file::LocalFileStore;
use umms_storage::graph::SqliteGraphStore;
use umms_storage::vector::LanceVectorStore;

/// Vector dimension for dev mode — small to keep things lightweight.
const DEV_VECTOR_DIM: usize = 8;

/// Known agent IDs used for stats and seeding.
const AGENT_IDS: &[&str] = &["coder", "researcher", "writer"];

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct AppState {
    cache: MokaMemoryCache,
    vector: LanceVectorStore,
    graph: SqliteGraphStore,
    #[allow(dead_code)]
    file_store: LocalFileStore,
    registry: prometheus_client::registry::Registry,
    started_at: Instant,
}

type SharedState = Arc<AppState>;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
    storage: StorageHealth,
}

#[derive(Serialize)]
struct StorageHealth {
    cache: &'static str,
    vector: &'static str,
    graph: &'static str,
    files: &'static str,
}

#[derive(Serialize)]
struct StatsResponse {
    cache: CacheStats,
    vector: VectorStats,
    graph: GraphStatsResponse,
    agents: Vec<String>,
}

#[derive(Serialize)]
struct CacheStats {
    l0_entries: usize,
    l1_entries: usize,
}

#[derive(Serialize)]
struct VectorStats {
    total_entries: u64,
}

#[derive(Serialize)]
struct GraphStatsResponse {
    total_nodes: u64,
    total_edges: u64,
    shared_nodes: u64,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn dashboard() -> Html<&'static str> {
    Html(include_str!("../../../dashboard/index.html"))
}

async fn health(State(state): State<SharedState>) -> Json<HealthResponse> {
    let uptime = state.started_at.elapsed().as_secs();

    // Simple liveness checks — if we got this far, the stores are alive.
    // A real health check would ping each backend; for dev this suffices.
    Json(HealthResponse {
        status: "healthy",
        uptime_secs: uptime,
        storage: StorageHealth {
            cache: "ok",
            vector: "ok",
            graph: "ok",
            files: "ok",
        },
    })
}

async fn stats(State(state): State<SharedState>) -> Json<StatsResponse> {
    // Cache: sum entries across known agents
    let mut l0_total = 0usize;
    let mut l1_total = 0usize;
    for name in AGENT_IDS {
        if let Ok(aid) = AgentId::from_str(name) {
            let entries = state.cache.entries_for_agent(&aid).await;
            for e in &entries {
                match e.layer {
                    MemoryLayer::SensoryBuffer => l0_total += 1,
                    MemoryLayer::WorkingMemory => l1_total += 1,
                    _ => {}
                }
            }
        }
    }

    // Vector: count with a dummy agent and include_shared = true
    let vector_total = if let Ok(aid) = AgentId::from_str("_global") {
        state.vector.count(&aid, true).await.unwrap_or(0)
    } else {
        0
    };

    // Graph: overall stats
    let gs = state.graph.stats(None).await.unwrap_or_default();

    // Collect agent names that have data
    let agents: Vec<String> = AGENT_IDS.iter().map(|s| (*s).to_owned()).collect();

    Json(StatsResponse {
        cache: CacheStats {
            l0_entries: l0_total,
            l1_entries: l1_total,
        },
        vector: VectorStats {
            total_entries: vector_total,
        },
        graph: GraphStatsResponse {
            total_nodes: gs.node_count,
            total_edges: gs.edge_count,
            shared_nodes: gs.shared_node_count,
        },
        agents,
    })
}

async fn metrics(State(state): State<SharedState>) -> String {
    umms_observe::encode_metrics(&state.registry)
}

async fn seed(State(state): State<SharedState>) -> Json<Value> {
    let mut total_memories = 0u64;

    for (idx, name) in AGENT_IDS.iter().enumerate() {
        let agent_id = match AgentId::from_str(name) {
            Ok(id) => id,
            Err(_) => continue,
        };

        // Insert 5 sample memories per agent into the vector store
        for i in 0..5 {
            let mut vec = vec![0.0f32; DEV_VECTOR_DIM];
            // Create pseudo-random-ish vectors based on agent and index
            for (d, v) in vec.iter_mut().enumerate() {
                *v = ((idx * 7 + i * 3 + d * 11) % 100) as f32 / 100.0;
            }

            let entry = MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
                .layer(MemoryLayer::EpisodicMemory)
                .scope(if i == 0 {
                    IsolationScope::Shared
                } else {
                    IsolationScope::Private
                })
                .content_text(format!("Sample memory {i} for agent {name}"))
                .vector(vec)
                .importance(0.5 + (i as f32) * 0.1)
                .tags(vec![format!("demo"), format!("agent-{name}")])
                .build();

            if state.vector.insert(&entry).await.is_ok() {
                total_memories += 1;
            }
        }
    }

    // Create graph nodes and edges
    let mut node_ids: Vec<NodeId> = Vec::new();
    let concepts = [
        ("Rust", KgNodeType::Concept),
        ("Memory Systems", KgNodeType::Concept),
        ("LanceDB", KgNodeType::Entity),
        ("SQLite", KgNodeType::Entity),
        ("Vector Search", KgNodeType::Concept),
        ("Knowledge Graph", KgNodeType::Concept),
    ];

    for (label, node_type) in &concepts {
        let node = KgNode {
            id: NodeId::new(),
            agent_id: None, // shared
            node_type: *node_type,
            label: (*label).to_owned(),
            properties: json!({"source": "dev-seed"}),
            importance: 0.8,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        if let Ok(nid) = state.graph.add_node(&node).await {
            node_ids.push(nid);
        }
    }

    let mut edge_count = 0u64;
    let edges = [
        (0, 1, "powers"),
        (2, 4, "enables"),
        (3, 5, "stores"),
        (1, 4, "includes"),
        (1, 5, "includes"),
    ];
    for (src, tgt, rel) in &edges {
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
                edge_count += 1;
            }
        }
    }

    Json(json!({
        "seeded": true,
        "memories": total_memories,
        "nodes": node_ids.len(),
        "edges": edge_count,
    }))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Init tracing
    umms_observe::init_tracing("info", false);

    // Init metrics
    let registry = umms_observe::init_metrics();

    // Determine data directory: ~/.umms/dev/
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_owned());
    let data_dir = std::path::PathBuf::from(home).join(".umms").join("dev");
    std::fs::create_dir_all(&data_dir).expect("failed to create data directory");

    tracing::info!(?data_dir, "UMMS dev data directory");

    // Create storage backends
    let cache = MokaMemoryCache::new();

    let vector_path = data_dir.join("lance");
    let vector = LanceVectorStore::new(
        vector_path.to_str().expect("non-UTF-8 path"),
        DEV_VECTOR_DIM,
    )
    .await
    .expect("failed to create LanceVectorStore");

    let graph_path = data_dir.join("graph.sqlite");
    let graph =
        SqliteGraphStore::new(&graph_path).expect("failed to create SqliteGraphStore");

    let files_path = data_dir.join("files");
    let file_store = LocalFileStore::new(files_path)
        .await
        .expect("failed to create LocalFileStore");

    // Shared state
    let state: SharedState = Arc::new(AppState {
        cache,
        vector,
        graph,
        file_store,
        registry,
        started_at: Instant::now(),
    });

    // Build router
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/health", get(health))
        .route("/api/stats", get(stats))
        .route("/api/metrics", get(metrics))
        .route("/api/demo/seed", get(seed))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let addr = "127.0.0.1:8720";
    println!("UMMS Dev Dashboard: http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind to port 8720");
    axum::serve(listener, app).await.expect("server error");
}
