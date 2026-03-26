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
