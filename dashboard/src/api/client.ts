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
  AgentListResponse,
  AgentPersonaResponse,
  CreateAgentRequest,
  UpdateAgentRequest,
  DeleteAgentResponse,
  AuditResponse,
  BenchmarksResponse,
  SeedResponse,
  EncoderStatusResponse,
  SemanticSearchResponse,
  IngestResponse,
  TagListResponse,
  TagSearchResponse,
  CooccurrenceResponse,
  EpaAnalyzeResponse,
  ConsolidationReportResponse,
  TraceListResponse,
  TraceSummaryResponse,
  AgentPromptConfig,
  PromptBlock,
  PromptMode,
  PromptWarehouse,
  PromptVariable,
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
// Agent Persona (M7)
// ---------------------------------------------------------------------------

export async function listAgents(): Promise<AgentListResponse> {
  if (IS_TAURI) return tauriInvoke('list_agents')
  return httpGet('/api/agents')
}

export async function getAgent(agentId: string): Promise<AgentPersonaResponse> {
  if (IS_TAURI) return tauriInvoke('get_agent', { agentId })
  return httpGet(`/api/agents/${agentId}`)
}

export async function createAgent(req: CreateAgentRequest): Promise<AgentPersonaResponse> {
  if (IS_TAURI) {
    return tauriInvoke('create_agent', {
      agentId: req.agent_id,
      name: req.name,
      role: req.role,
      description: req.description,
      expertise: req.expertise,
    })
  }
  const res = await fetch('/api/agents', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

export async function updateAgent(agentId: string, req: UpdateAgentRequest): Promise<AgentPersonaResponse> {
  if (IS_TAURI) {
    return tauriInvoke('update_agent', {
      agentId,
      name: req.name,
      role: req.role,
      description: req.description,
      expertise: req.expertise,
      systemPrompt: req.system_prompt,
    })
  }
  const res = await fetch(`/api/agents/${agentId}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

export async function deleteAgent(agentId: string): Promise<DeleteAgentResponse> {
  if (IS_TAURI) return tauriInvoke('delete_agent', { agentId })
  const res = await fetch(`/api/agents/${agentId}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
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

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

export async function listTags(agentId: string): Promise<TagListResponse> {
  if (IS_TAURI) return tauriInvoke('list_tags', { agentId })
  return httpGet(`/api/tags/${agentId}`)
}

export async function searchTags(
  query: string,
  agentId?: string,
  topK = 10,
): Promise<TagSearchResponse> {
  if (IS_TAURI) return tauriInvoke('search_tags', { query, agentId, topK })
  const res = await fetch('/api/tags/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, agent_id: agentId, top_k: topK }),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

export async function getTagCooccurrences(tagId: string): Promise<CooccurrenceResponse> {
  if (IS_TAURI) return tauriInvoke('tag_cooccurrences', { tagId })
  return httpGet(`/api/tags/cooccurrences/${tagId}`)
}

// ---------------------------------------------------------------------------
// EPA
// ---------------------------------------------------------------------------

export async function epaAnalyze(
  query: string,
  agentId?: string,
): Promise<EpaAnalyzeResponse> {
  if (IS_TAURI) return tauriInvoke('epa_analyze', { query, agentId })
  const res = await fetch('/api/epa/analyze', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, agent_id: agentId }),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

// ---------------------------------------------------------------------------
// Consolidation
// ---------------------------------------------------------------------------

export async function runConsolidation(agentId: string): Promise<ConsolidationReportResponse> {
  if (IS_TAURI) return tauriInvoke('run_consolidation', { agentId })
  const res = await fetch(`/api/consolidation/run/${agentId}`, {
    method: 'POST',
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

// ---------------------------------------------------------------------------
// Encoder (continued)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Model Traces
// ---------------------------------------------------------------------------

export async function listTraces(
  limit?: number,
  modelId?: string,
  task?: string,
): Promise<TraceListResponse> {
  if (IS_TAURI) return tauriInvoke('list_traces', { limit, modelId, task })
  const qs = new URLSearchParams()
  if (limit) qs.set('limit', String(limit))
  if (modelId) qs.set('model_id', modelId)
  if (task) qs.set('task', task)
  return httpGet(`/api/traces?${qs}`)
}

export async function traceSummary(): Promise<TraceSummaryResponse> {
  if (IS_TAURI) return tauriInvoke('trace_summary')
  return httpGet('/api/traces/summary')
}

export async function clearTraces(): Promise<{ cleared: boolean }> {
  if (IS_TAURI) return tauriInvoke('clear_traces')
  const res = await fetch('/api/traces', { method: 'DELETE' })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

// ---------------------------------------------------------------------------
// Prompt Editor
// ---------------------------------------------------------------------------

async function httpPost<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

async function httpPut<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

async function httpDelete<T>(path: string): Promise<T> {
  const res = await fetch(path, { method: 'DELETE' })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

export async function getPromptConfig(agentId: string): Promise<AgentPromptConfig> {
  if (IS_TAURI) return tauriInvoke('get_prompt_config', { agentId })
  return httpGet(`/api/prompts/${agentId}`)
}

export async function savePromptConfig(agentId: string, config: AgentPromptConfig): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('save_prompt_config', { agentId, config })
    return
  }
  await httpPut(`/api/prompts/${agentId}`, {
    mode: config.mode,
    original_prompt: config.original_prompt,
    blocks: config.blocks,
    preset_path: config.preset_path,
    preset_content: config.preset_content,
  })
}

export async function switchPromptMode(agentId: string, mode: PromptMode): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('switch_prompt_mode', { agentId, mode })
    return
  }
  await httpPut(`/api/prompts/${agentId}/mode`, { mode })
}

export async function addBlock(agentId: string, block: PromptBlock): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('add_prompt_block', { agentId, block })
    return
  }
  await httpPost(`/api/prompts/${agentId}/blocks`, block)
}

export async function updateBlock(agentId: string, blockId: string, block: Partial<PromptBlock>): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('update_prompt_block', { agentId, blockId, block })
    return
  }
  await httpPut(`/api/prompts/${agentId}/blocks/${blockId}`, block)
}

export async function deleteBlock(agentId: string, blockId: string): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('delete_prompt_block', { agentId, blockId })
    return
  }
  await httpDelete(`/api/prompts/${agentId}/blocks/${blockId}`)
}

export async function reorderBlocks(agentId: string, blockIds: string[]): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('reorder_prompt_blocks', { agentId, blockIds })
    return
  }
  await httpPut(`/api/prompts/${agentId}/blocks/reorder`, { block_ids: blockIds })
}

export async function addVariant(agentId: string, blockId: string, content: string): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('add_prompt_variant', { agentId, blockId, content })
    return
  }
  await httpPost(`/api/prompts/${agentId}/blocks/${blockId}/variants`, { content })
}

export async function selectVariant(agentId: string, blockId: string, index: number): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('select_prompt_variant', { agentId, blockId, index })
    return
  }
  await httpPut(`/api/prompts/${agentId}/blocks/${blockId}/variant/${index}`, {})
}

export async function listWarehouses(): Promise<PromptWarehouse[]> {
  if (IS_TAURI) return tauriInvoke('list_prompt_warehouses')
  const resp: { warehouses: PromptWarehouse[] } = await httpGet('/api/prompts/warehouses')
  return resp.warehouses
}

export async function getWarehouse(name: string): Promise<PromptWarehouse> {
  if (IS_TAURI) return tauriInvoke('get_prompt_warehouse', { name })
  return httpGet(`/api/prompts/warehouses/${encodeURIComponent(name)}`)
}

export async function saveWarehouse(warehouse: PromptWarehouse): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('save_prompt_warehouse', { warehouse })
    return
  }
  await httpPut(`/api/prompts/warehouses/${encodeURIComponent(warehouse.name)}`, {
    name: warehouse.name,
    blocks: warehouse.blocks,
    is_global: warehouse.is_global,
  })
}

export async function deleteWarehouse(name: string): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke('delete_prompt_warehouse', { name })
    return
  }
  await httpDelete(`/api/prompts/warehouses/${encodeURIComponent(name)}`)
}

export async function listVariables(): Promise<PromptVariable[]> {
  if (IS_TAURI) return tauriInvoke('list_prompt_variables')
  const resp: { variables: PromptVariable[] } = await httpGet('/api/prompts/variables')
  return resp.variables
}

export async function previewPrompt(agentId: string): Promise<string> {
  if (IS_TAURI) return tauriInvoke('preview_prompt', { agentId })
  const resp: { resolved_prompt: string } = await httpPost('/api/prompts/preview', { agent_id: agentId })
  return resp.resolved_prompt
}
