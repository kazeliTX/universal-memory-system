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
// Agent Persona (M7)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AgentPersonaResponse {
    pub agent_id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub expertise: Vec<String>,
    pub retrieval_config: AgentRetrievalConfigResponse,
    pub created_at: String,
    pub updated_at: String,
    // stats
    pub cache_l0: usize,
    pub cache_l1: usize,
    pub vector_count: u64,
}

#[derive(Debug, Serialize)]
pub struct AgentRetrievalConfigResponse {
    pub bm25_weight: Option<f32>,
    pub min_score: Option<f32>,
    pub top_k_final: Option<usize>,
    pub lif_hops: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct AgentListResponse {
    pub agents: Vec<AgentPersonaResponse>,
}

#[derive(Debug, Serialize)]
pub struct DeleteAgentResponse {
    pub deleted: bool,
    pub agent_id: String,
    pub had_memories: bool,
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
// Semantic search — full pipeline visualization
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SemanticSearchResponse {
    pub query: String,
    pub results: Vec<SearchHit>,
    /// Per-stage latency breakdown.
    pub latency: PipelineLatency,
    /// Pipeline stage statistics.
    pub pipeline: PipelineStats,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub entry: MemoryEntry,
    /// Final fused score after all stages.
    pub score: f32,
    /// How this entry was discovered.
    pub source: String,
    /// Rank in BM25 results (None if not found by BM25).
    pub bm25_rank: Option<usize>,
    /// Rank in vector results (None if not found by vector search).
    pub vector_rank: Option<usize>,
    /// RRF contribution from BM25 side.
    pub bm25_contribution: f32,
    /// RRF contribution from vector side.
    pub vector_contribution: f32,
}

#[derive(Debug, Default, Serialize)]
pub struct PipelineLatency {
    pub encode_ms: u64,
    pub recall_ms: u64,
    pub rerank_ms: u64,
    pub diffusion_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct PipelineStats {
    /// Candidates from hybrid recall (before rerank).
    pub recall_count: usize,
    /// Candidates after rerank.
    pub rerank_count: usize,
    /// Entries discovered by graph diffusion.
    pub diffusion_count: usize,
    /// Final result count.
    pub final_count: usize,
    /// BM25-only hits (not in vector results).
    pub bm25_only: usize,
    /// Vector-only hits (not in BM25 results).
    pub vector_only: usize,
    /// Both BM25 and vector hits.
    pub both: usize,
}

// ---------------------------------------------------------------------------
// Document ingestion
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub chunks_created: usize,
    pub chunks_stored: usize,
    pub title: String,
    pub summary: String,
    pub total_ms: u64,
    pub latency: IngestLatencyResponse,
    pub chunks: Vec<ChunkDetailResponse>,
    pub graph_nodes_created: usize,
    pub graph_edges_created: usize,
}

#[derive(Debug, Serialize)]
pub struct IngestLatencyResponse {
    pub chunk_ms: u64,
    pub skeleton_ms: u64,
    pub encode_ms: u64,
    pub store_ms: u64,
    pub graph_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ChunkDetailResponse {
    pub index: usize,
    pub original_text: String,
    pub context_prefix: String,
    pub section: String,
    pub tags: Vec<String>,
    pub memory_id: String,
    pub char_count: usize,
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TagListResponse {
    pub agent_id: String,
    pub tags: Vec<TagResponse>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct TagResponse {
    pub id: String,
    pub label: String,
    pub canonical: String,
    pub frequency: u64,
    pub importance: f32,
}

#[derive(Debug, Serialize)]
pub struct TagSearchResponse {
    pub results: Vec<TagMatchResponse>,
}

#[derive(Debug, Serialize)]
pub struct TagMatchResponse {
    pub tag: TagResponse,
    pub similarity: f32,
}

#[derive(Debug, Serialize)]
pub struct CooccurrenceResponse {
    pub tag_id: String,
    pub cooccurrences: Vec<CoocEntry>,
}

#[derive(Debug, Serialize)]
pub struct CoocEntry {
    pub partner_tag: TagResponse,
    pub count: u64,
    pub pmi: f32,
}

// ---------------------------------------------------------------------------
// EPA
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct EpaAnalyzeResponse {
    pub logic_depth: f32,
    pub cross_domain_resonance: f32,
    pub activated_tags: Vec<ActivatedTagResponse>,
    pub alpha: f32,
    pub num_semantic_axes: usize,
}

#[derive(Debug, Serialize)]
pub struct ActivatedTagResponse {
    pub tag_id: String,
    pub label: String,
    pub similarity: f32,
}

// ---------------------------------------------------------------------------
// Consolidation (M4)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ConsolidationReportResponse {
    pub agent_id: String,
    pub decay: DecayResultResponse,
    pub evolution: EvolutionResultResponse,
    pub promotion: PromoteResultResponse,
    pub total_ms: u64,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct DecayResultResponse {
    pub scanned: usize,
    pub updated: usize,
    pub archived: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct EvolutionResultResponse {
    pub pairs_scanned: usize,
    pub nodes_merged: usize,
    pub edges_strengthened: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct PromoteResultResponse {
    pub scanned: usize,
    pub promoted: usize,
    pub elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// Models (M5 model pool)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub pool_available: bool,
    pub models: Vec<ModelInfoResponse>,
}

#[derive(Debug, Serialize)]
pub struct ModelInfoResponse {
    pub id: String,
    pub provider: String,
    pub model_name: String,
    pub tasks: Vec<String>,
    pub dimension: Option<usize>,
    pub max_tokens: Option<usize>,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ModelStatsResponse>,
}

#[derive(Debug, Serialize)]
pub struct ModelStatsResponse {
    pub total_requests: u64,
    pub total_errors: u64,
    pub avg_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ScheduledTaskResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub task_type: String,
    pub schedule: String,
    pub enabled: bool,
    pub params: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<ScheduledTaskResponse>,
}

#[derive(Debug, Serialize)]
pub struct TaskExecutionResponse {
    pub id: String,
    pub task_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub result: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ExecutionListResponse {
    pub executions: Vec<TaskExecutionResponse>,
}

#[derive(Debug, Serialize)]
pub struct TriggerTaskResponse {
    pub execution_id: String,
    pub message: String,
}

/// Convert a `ScheduledTask` to the API response type.
impl From<umms_scheduler::ScheduledTask> for ScheduledTaskResponse {
    fn from(t: umms_scheduler::ScheduledTask) -> Self {
        Self {
            id: t.id,
            name: t.name,
            description: t.description,
            task_type: t.task_type.to_string(),
            schedule: t.schedule.to_string(),
            enabled: t.enabled,
            params: t.params,
            created_at: t.created_at.to_rfc3339(),
            updated_at: t.updated_at.to_rfc3339(),
            last_run_at: t.last_run_at.map(|t| t.to_rfc3339()),
            next_run_at: t.next_run_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// Convert a `TaskExecution` to the API response type.
impl From<umms_scheduler::TaskExecution> for TaskExecutionResponse {
    fn from(e: umms_scheduler::TaskExecution) -> Self {
        Self {
            id: e.id,
            task_id: e.task_id,
            started_at: e.started_at.to_rfc3339(),
            finished_at: e.finished_at.map(|t| t.to_rfc3339()),
            status: e.status.to_string(),
            result: e.result,
        }
    }
}

// ---------------------------------------------------------------------------
// Diary
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DiaryEntryResponse {
    pub id: String,
    pub agent_id: String,
    pub category: String,
    pub content: String,
    pub confidence: f32,
    pub source_session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct DiaryListResponse {
    pub agent_id: String,
    pub entries: Vec<DiaryEntryResponse>,
    pub total: usize,
}

impl From<umms_persona::DiaryEntry> for DiaryEntryResponse {
    fn from(e: umms_persona::DiaryEntry) -> Self {
        Self {
            id: e.id,
            agent_id: e.agent_id,
            category: e.category.to_string(),
            content: e.content,
            confidence: e.confidence,
            source_session_id: e.source_session_id,
            created_at: e.created_at.to_rfc3339(),
            updated_at: e.updated_at.to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub message: String,
    pub agent_id: String,
    pub session_id: String,
    pub sources: Vec<ChatSource>,
    pub latency_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ChatSource {
    pub content: String,
    pub score: f32,
    pub memory_id: String,
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionSummaryResponse>,
}

#[derive(Debug, Serialize)]
pub struct SessionSummaryResponse {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub message_count: usize,
    pub last_message_preview: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SessionDetailResponse {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub messages: Vec<SessionMessageResponse>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SessionMessageResponse {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub sources: Vec<ChatSource>,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Model Traces
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TraceListResponse {
    pub traces: Vec<ModelTraceResponse>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ModelTraceResponse {
    pub id: String,
    pub timestamp: String,
    pub model_id: String,
    pub model_name: String,
    pub provider: String,
    pub task: String,
    pub request_type: String,
    pub input_preview: String,
    pub input_tokens_estimate: usize,
    pub success: bool,
    pub error_message: Option<String>,
    pub output_preview: Option<String>,
    pub output_dimension: Option<usize>,
    pub output_tokens_estimate: Option<usize>,
    pub latency_ms: u64,
    pub retry_count: u32,
    pub caller: String,
}

#[derive(Debug, Serialize)]
pub struct TraceSummaryResponse {
    pub total_traces: usize,
    pub total_errors: usize,
    pub by_model: Vec<ModelTraceStatResponse>,
    pub by_task: Vec<TaskTraceStatResponse>,
    pub avg_latency_ms: f64,
    pub p99_latency_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct ModelTraceStatResponse {
    pub model_id: String,
    pub count: usize,
    pub errors: usize,
    pub avg_latency_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct TaskTraceStatResponse {
    pub task: String,
    pub count: usize,
    pub errors: usize,
    pub avg_latency_ms: f64,
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
