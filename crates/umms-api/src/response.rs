//! Shared response types for all API endpoints.
//!
//! These types are serialised to JSON for HTTP responses and also returned
//! directly from Tauri Commands. Keeping them in one place ensures the HTTP
//! and IPC interfaces always agree on the shape of the data.

use serde::Serialize;

use umms_core::types::{GraphStats, KgEdge, KgNode, MemoryEntry};
use umms_observe::AuditEvent;

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub uptime_secs: u64,
    pub storage: StorageHealth,
}

#[derive(Debug, Serialize)]
pub struct StorageHealth {
    pub cache: &'static str,
    pub vector: &'static str,
    pub graph: &'static str,
    pub files: &'static str,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub cache: CacheStats,
    pub vector: VectorStats,
    pub graph: GraphStatsDto,
    pub agents: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CacheStats {
    pub l0_entries: usize,
    pub l1_entries: usize,
}

#[derive(Debug, Serialize)]
pub struct VectorStats {
    pub total_entries: u64,
}

#[derive(Debug, Serialize)]
pub struct GraphStatsDto {
    pub total_nodes: u64,
    pub total_edges: u64,
    pub shared_nodes: u64,
    pub shared_edges: u64,
}

impl From<GraphStats> for GraphStatsDto {
    fn from(gs: GraphStats) -> Self {
        Self {
            total_nodes: gs.node_count,
            total_edges: gs.edge_count,
            shared_nodes: gs.shared_node_count,
            shared_edges: gs.shared_edge_count,
        }
    }
}

// ---------------------------------------------------------------------------
// Memory browsing
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CacheEntriesResponse {
    pub agent_id: String,
    pub l0: Vec<MemoryEntry>,
    pub l1: Vec<MemoryEntry>,
}

#[derive(Debug, Serialize)]
pub struct VectorEntriesResponse {
    pub agent_id: String,
    pub entries: Vec<MemoryEntry>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Serialize)]
pub struct MemoryDetailResponse {
    pub entry: MemoryEntry,
}

// ---------------------------------------------------------------------------
// Knowledge graph
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct GraphNodesResponse {
    pub agent_id: String,
    pub nodes: Vec<KgNode>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct NodeDetailResponse {
    pub node: KgNode,
    pub edges: Vec<KgEdge>,
}

#[derive(Debug, Serialize)]
pub struct TraverseResponse {
    pub nodes: Vec<KgNode>,
    pub edges: Vec<KgEdge>,
}

#[derive(Debug, Serialize)]
pub struct GraphSearchResponse {
    pub nodes: Vec<KgNode>,
    pub query: String,
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct FileListResponse {
    pub agent_id: String,
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AgentDetailResponse {
    pub agent_id: String,
    pub cache_l0: usize,
    pub cache_l1: usize,
    pub vector_count: u64,
    pub graph: GraphStatsDto,
    pub file_count: usize,
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AuditResponse {
    pub events: Vec<AuditEvent>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct BenchmarkEntry {
    pub name: String,
    pub mean_ns: f64,
    pub median_ns: f64,
    pub std_dev_ns: f64,
}

#[derive(Debug, Serialize)]
pub struct BenchmarksResponse {
    pub benchmarks: Vec<BenchmarkEntry>,
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct EncoderStatusResponse {
    pub available: bool,
    pub model: Option<String>,
    pub dimension: Option<usize>,
    pub total_requests: u64,
    pub total_texts_encoded: u64,
    pub total_errors: u64,
    pub total_retries: u64,
    pub avg_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// Semantic search
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SemanticSearchResponse {
    pub query: String,
    pub results: Vec<SearchHit>,
    pub latency_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub entry: MemoryEntry,
    pub score: f32,
}

// ---------------------------------------------------------------------------
// Seed
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SeedResponse {
    pub seeded: bool,
    pub memories: u64,
    pub nodes: usize,
    pub edges: u64,
}

// ---------------------------------------------------------------------------
// Clear
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ClearResponse {
    pub cleared: bool,
    pub vectors_deleted: u64,
    pub nodes_deleted: u64,
    pub edges_deleted: u64,
    pub cache_evicted: usize,
}
