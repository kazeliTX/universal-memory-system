/**
 * Unified API client — automatically switches between Tauri IPC and HTTP.
 *
 * Vue components call these functions without knowing the transport layer.
 * In Tauri: uses `invoke()` for sub-millisecond IPC.
 * In browser: falls back to `fetch()` against the Axum HTTP server.
 */

import type {
  HealthResponse,
  StatsResponse,
  CacheEntriesResponse,
  VectorEntriesResponse,
  MemoryDetailResponse,
  GraphNodesResponse,
  NodeDetailResponse,
  TraverseResponse,
  GraphSearchResponse,
  FileListResponse,
  AgentDetailResponse,
  AuditResponse,
  BenchmarksResponse,
  SeedResponse,
  EncoderStatusResponse,
  SemanticSearchResponse,
  IngestResponse,
} from '@/types'

// ---------------------------------------------------------------------------
// Transport detection
// ---------------------------------------------------------------------------

const IS_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

async function httpGet<T>(path: string): Promise<T> {
  const res = await fetch(path)
  if (!res.ok) {
    const body = await res.text()
    throw new Error(`HTTP ${res.status}: ${body}`)
  }
  return res.json()
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

export async function getHealth(): Promise<HealthResponse> {
  if (IS_TAURI) return tauriInvoke('get_health')
  return httpGet('/api/health')
}

export async function getStats(): Promise<StatsResponse> {
  if (IS_TAURI) return tauriInvoke('get_stats')
  return httpGet('/api/stats')
}

export async function getMetrics(): Promise<string> {
  if (IS_TAURI) return tauriInvoke('get_metrics')
  const res = await fetch('/api/metrics')
  return res.text()
}

export async function seedDemo(): Promise<SeedResponse> {
  return httpGet('/api/demo/seed') // Always HTTP — seed modifies state via Axum
}

export async function clearDemo(): Promise<{ cleared: boolean }> {
  return httpGet('/api/demo/clear')
}

// ---------------------------------------------------------------------------
// Memory browsing
// ---------------------------------------------------------------------------

export async function getCacheEntries(agentId: string): Promise<CacheEntriesResponse> {
  if (IS_TAURI) return tauriInvoke('get_cache_entries', { agentId })
  return httpGet(`/api/memories/cache/${agentId}`)
}

export async function getVectorEntries(
  agentId: string,
  offset = 0,
  limit = 20,
  includeShared = true,
): Promise<VectorEntriesResponse> {
  if (IS_TAURI) return tauriInvoke('list_vector_entries', { agentId, offset, limit, includeShared })
  return httpGet(`/api/memories/vector/${agentId}?offset=${offset}&limit=${limit}&include_shared=${includeShared}`)
}

export async function getMemoryDetail(memoryId: string): Promise<MemoryDetailResponse> {
  if (IS_TAURI) return tauriInvoke('get_memory_detail', { memoryId })
  return httpGet(`/api/memories/vector/entry/${memoryId}`)
}

// ---------------------------------------------------------------------------
// Knowledge graph
// ---------------------------------------------------------------------------

export async function getGraphNodes(agentId: string, limit = 50): Promise<GraphNodesResponse> {
  if (IS_TAURI) return tauriInvoke('list_graph_nodes', { agentId, limit })
  return httpGet(`/api/memories/graph/${agentId}?limit=${limit}`)
}

export async function getNodeDetail(nodeId: string): Promise<NodeDetailResponse> {
  if (IS_TAURI) return tauriInvoke('get_node_detail', { nodeId })
  return httpGet(`/api/memories/graph/node/${nodeId}`)
}

export async function traverseGraph(
  nodeId: string,
  hops = 2,
  agentId?: string,
): Promise<TraverseResponse> {
  if (IS_TAURI) return tauriInvoke('traverse_graph', { nodeId, hops, agentId })
  const params = new URLSearchParams({ hops: String(hops) })
  if (agentId) params.set('agent_id', agentId)
  return httpGet(`/api/memories/graph/traverse/${nodeId}?${params}`)
}

export async function searchGraph(
  query: string,
  agentId?: string,
  limit = 10,
): Promise<GraphSearchResponse> {
  if (IS_TAURI) return tauriInvoke('search_graph', { query, agentId, limit })
  const params = new URLSearchParams({ q: query, limit: String(limit) })
  if (agentId) params.set('agent_id', agentId)
  return httpGet(`/api/memories/graph/search?${params}`)
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

export async function getFileList(agentId: string): Promise<FileListResponse> {
  if (IS_TAURI) return tauriInvoke('list_files', { agentId })
  return httpGet(`/api/memories/files/${agentId}`)
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

export async function getAgentDetail(agentId: string): Promise<AgentDetailResponse> {
  if (IS_TAURI) return tauriInvoke('get_agent_detail', { agentId })
  return httpGet(`/api/agents/${agentId}`)
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

export async function getAuditEvents(params?: {
  agentId?: string
  eventType?: string
  limit?: number
  offset?: number
}): Promise<AuditResponse> {
  if (IS_TAURI) {
    return tauriInvoke('query_audit_events', {
      agentId: params?.agentId,
      eventType: params?.eventType,
      limit: params?.limit,
      offset: params?.offset,
    })
  }
  const qs = new URLSearchParams()
  if (params?.agentId) qs.set('agent_id', params.agentId)
  if (params?.eventType) qs.set('event_type', params.eventType)
  if (params?.limit) qs.set('limit', String(params.limit))
  if (params?.offset) qs.set('offset', String(params.offset))
  return httpGet(`/api/audit?${qs}`)
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

export async function getBenchmarks(): Promise<BenchmarksResponse> {
  return httpGet('/api/benchmarks') // Always HTTP — reads files from disk
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

export async function getEncoderStatus(): Promise<EncoderStatusResponse> {
  if (IS_TAURI) return tauriInvoke('encoder_status')
  return httpGet('/api/encoder/status')
}

export async function semanticSearch(
  query: string,
  agentId?: string,
  topK = 5,
  includeShared = true,
): Promise<SemanticSearchResponse> {
  if (IS_TAURI) {
    return tauriInvoke('semantic_search', { query, agentId, topK, includeShared })
  }
  const res = await fetch('/api/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      query,
      agent_id: agentId,
      top_k: topK,
      include_shared: includeShared,
    }),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

export async function ingestDocument(
  text: string,
  agentId = 'coder',
  scope = 'private',
  tags: string[] = [],
): Promise<IngestResponse> {
  if (IS_TAURI) {
    return tauriInvoke('ingest_document', { text, agentId, scope, tags })
  }
  const res = await fetch('/api/ingest', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      text,
      agent_id: agentId,
      scope,
      tags,
    }),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

export async function encodeText(text: string): Promise<{ vector: number[] }> {
  if (IS_TAURI) {
    const vector = await tauriInvoke<number[]>('encode_text', { text })
    return { vector }
  }
  const res = await fetch('/api/encode', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ text }),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}
