/**
 * TypeScript types mirroring Rust structs from umms-core and umms-api.
 *
 * Keep these in sync with:
 * - crates/umms-core/src/types.rs (MemoryEntry, KgNode, KgEdge, etc.)
 * - crates/umms-api/src/response.rs (API response types)
 * - crates/umms-observe/src/audit.rs (AuditEvent, AuditEventType)
 */

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

export interface MemoryEntry {
  id: string
  agent_id: string
  layer: MemoryLayer
  scope: IsolationScope
  modality: Modality
  content_text: string | null
  vector: number[] | null
  importance: number
  decay_category: DecayCategory
  tags: string[]
  metadata: Record<string, unknown>
  created_at: string
  accessed_at: string
  access_count: number
}

export type MemoryLayer =
  | 'SensoryBuffer'
  | 'WorkingMemory'
  | 'EpisodicMemory'
  | 'SemanticMemory'
  | 'RawStorage'

export type IsolationScope = 'Private' | 'Shared' | 'External'

export type Modality = 'Text' | 'Image' | 'Audio' | 'Code' | 'File'

export type DecayCategory =
  | 'TaskContext'
  | 'SessionTopic'
  | 'UserPreference'
  | 'DomainKnowledge'

export interface KgNode {
  id: string
  agent_id: string | null
  node_type: KgNodeType
  label: string
  properties: Record<string, unknown>
  importance: number
  created_at: string
  updated_at: string
}

export type KgNodeType = 'Entity' | 'Concept' | 'Relation'

export interface KgEdge {
  id: string
  source_id: string
  target_id: string
  relation: string
  weight: number
  agent_id: string | null
  created_at: string
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

export interface HealthResponse {
  status: string
  uptime_secs: number
  storage: {
    cache: string
    vector: string
    graph: string
    files: string
  }
}

export interface StatsResponse {
  cache: { l0_entries: number; l1_entries: number }
  vector: { total_entries: number }
  graph: GraphStatsDto
  agents: string[]
}

export interface GraphStatsDto {
  total_nodes: number
  total_edges: number
  shared_nodes: number
  shared_edges: number
}

export interface CacheEntriesResponse {
  agent_id: string
  l0: MemoryEntry[]
  l1: MemoryEntry[]
}

export interface VectorEntriesResponse {
  agent_id: string
  entries: MemoryEntry[]
  total: number
  offset: number
  limit: number
}

export interface MemoryDetailResponse {
  entry: MemoryEntry
}

export interface GraphNodesResponse {
  agent_id: string
  nodes: KgNode[]
  total: number
}

export interface NodeDetailResponse {
  node: KgNode
  edges: KgEdge[]
}

export interface TraverseResponse {
  nodes: KgNode[]
  edges: KgEdge[]
}

export interface GraphSearchResponse {
  nodes: KgNode[]
  query: string
}

export interface FileListResponse {
  agent_id: string
  files: string[]
}

export interface AgentDetailResponse {
  agent_id: string
  cache_l0: number
  cache_l1: number
  vector_count: number
  graph: GraphStatsDto
  file_count: number
}

export interface AuditEvent {
  id: number
  timestamp: string
  event_type: string
  agent_id: string
  memory_id: string | null
  node_id: string | null
  layer: string | null
  details: unknown
}

export interface AuditResponse {
  events: AuditEvent[]
  total: number
}

export interface BenchmarkEntry {
  name: string
  mean_ns: number
  median_ns: number
  std_dev_ns: number
}

export interface BenchmarksResponse {
  benchmarks: BenchmarkEntry[]
}

export interface SeedResponse {
  seeded: boolean
  memories: number
  nodes: number
  edges: number
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

export interface EncoderStatusResponse {
  available: boolean
  model: string | null
  dimension: number | null
  total_requests: number
  total_texts_encoded: number
  total_errors: number
  total_retries: number
  avg_latency_ms: number
}

export interface EncodeResponse {
  vector: number[]
}

export interface IngestResponse {
  chunks_created: number
  chunks_stored: number
  title: string
  summary: string
  total_ms: number
  latency: {
    chunk_ms: number
    skeleton_ms: number
    encode_ms: number
    store_ms: number
  }
  chunks: ChunkDetail[]
}

export interface ChunkDetail {
  index: number
  original_text: string
  context_prefix: string
  section: string
  tags: string[]
  memory_id: string
  char_count: number
}

export interface SemanticSearchResponse {
  query: string
  results: SearchHit[]
  latency: PipelineLatency
  pipeline: PipelineStats
}

export interface SearchHit {
  entry: MemoryEntry
  score: number
  source: 'both' | 'bm25_only' | 'vector_only' | 'diffusion' | 'unknown'
  bm25_rank: number | null
  vector_rank: number | null
  bm25_contribution: number
  vector_contribution: number
}

export interface PipelineLatency {
  encode_ms: number
  recall_ms: number
  rerank_ms: number
  diffusion_ms: number
  total_ms: number
}

export interface PipelineStats {
  recall_count: number
  rerank_count: number
  diffusion_count: number
  final_count: number
  bm25_only: number
  vector_only: number
  both: number
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

export interface TagResponse {
  id: string
  label: string
  canonical: string
  frequency: number
  importance: number
}

export interface TagListResponse {
  agent_id: string
  tags: TagResponse[]
  total: number
}

export interface TagMatchResponse {
  tag: TagResponse
  similarity: number
}

export interface TagSearchResponse {
  results: TagMatchResponse[]
}

export interface CoocEntry {
  partner_tag: TagResponse
  count: number
  pmi: number
}

export interface CooccurrenceResponse {
  tag_id: string
  cooccurrences: CoocEntry[]
}

// ---------------------------------------------------------------------------
// Consolidation (M4)
// ---------------------------------------------------------------------------

export interface ConsolidationReportResponse {
  agent_id: string
  decay: DecayResultResponse
  evolution: EvolutionResultResponse
  promotion: PromoteResultResponse
  total_ms: number
  timestamp: string
}

export interface DecayResultResponse {
  scanned: number
  updated: number
  archived: number
  elapsed_ms: number
}

export interface EvolutionResultResponse {
  pairs_scanned: number
  nodes_merged: number
  edges_strengthened: number
  elapsed_ms: number
}

export interface PromoteResultResponse {
  scanned: number
  promoted: number
  elapsed_ms: number
}

// ---------------------------------------------------------------------------
// Agent Persona (M7)
// ---------------------------------------------------------------------------

export interface AgentRetrievalConfigResponse {
  bm25_weight: number | null
  min_score: number | null
  top_k_final: number | null
  lif_hops: number | null
}

export interface AgentPersonaResponse {
  agent_id: string
  name: string
  role: string
  description: string
  expertise: string[]
  retrieval_config: AgentRetrievalConfigResponse
  created_at: string
  updated_at: string
  cache_l0: number
  cache_l1: number
  vector_count: number
}

export interface AgentListResponse {
  agents: AgentPersonaResponse[]
}

export interface DeleteAgentResponse {
  deleted: boolean
  agent_id: string
  had_memories: boolean
}

export interface CreateAgentRequest {
  agent_id: string
  name: string
  role?: string
  description?: string
  expertise?: string[]
}

export interface UpdateAgentRequest {
  name?: string
  role?: string
  description?: string
  expertise?: string[]
  system_prompt?: string
}

// ---------------------------------------------------------------------------
// Graph visualization (force-directed)
// ---------------------------------------------------------------------------

export interface ForceGraphNode {
  id: string
  label: string
  group?: string
  size?: number
}

export interface ForceGraphLink {
  source: string
  target: string
  weight?: number
  label?: string
}

// ---------------------------------------------------------------------------
// EPA
// ---------------------------------------------------------------------------

export interface ActivatedTagResponse {
  tag_id: string
  label: string
  similarity: number
}

export interface EpaAnalyzeResponse {
  logic_depth: number
  cross_domain_resonance: number
  activated_tags: ActivatedTagResponse[]
  alpha: number
  num_semantic_axes: number
}

// ---------------------------------------------------------------------------
// Model Traces
// ---------------------------------------------------------------------------

export interface ModelTraceResponse {
  id: string
  timestamp: string
  model_id: string
  model_name: string
  provider: string
  task: string
  request_type: string
  input_preview: string
  input_tokens_estimate: number
  success: boolean
  error_message: string | null
  output_preview: string | null
  output_dimension: number | null
  output_tokens_estimate: number | null
  latency_ms: number
  retry_count: number
  caller: string
}

export interface TraceListResponse {
  traces: ModelTraceResponse[]
  total: number
}

export interface ModelTraceStatResponse {
  model_id: string
  count: number
  errors: number
  avg_latency_ms: number
}

export interface TaskTraceStatResponse {
  task: string
  count: number
  errors: number
  avg_latency_ms: number
}

export interface TraceSummaryResponse {
  total_traces: number
  total_errors: number
  by_model: ModelTraceStatResponse[]
  by_task: TaskTraceStatResponse[]
  avg_latency_ms: number
  p99_latency_ms: number
}

// ---------------------------------------------------------------------------
// Prompt Editor
// ---------------------------------------------------------------------------

export type PromptMode = 'original' | 'modular' | 'preset'

export interface PromptBlock {
  id: string
  name: string
  block_type: string
  content: string
  variants: string[]
  selected_variant: number
  enabled: boolean
  order: number
}

export interface AgentPromptConfig {
  agent_id: string
  mode: PromptMode
  original_prompt: string
  blocks: PromptBlock[]
  preset_path?: string
  preset_content?: string
  updated_at: string
}

export interface PromptWarehouse {
  name: string
  blocks: PromptBlock[]
  is_global: boolean
}

export interface PromptVariable {
  name: string
  description: string
  resolver: string
}
