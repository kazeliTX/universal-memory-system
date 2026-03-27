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
// Prompt Editor (mock implementations — backend pending)
// ---------------------------------------------------------------------------

const MOCK_VARIABLES: PromptVariable[] = [
  { name: 'AgentName', description: '当前智能体名称', resolver: 'agent.name' },
  { name: 'AgentRole', description: '智能体角色', resolver: 'agent.role' },
  { name: 'DateTime', description: '当前日期时间', resolver: 'system.datetime' },
  { name: 'Date', description: '当前日期', resolver: 'system.date' },
  { name: 'UserLanguage', description: '用户语言偏好', resolver: 'user.language' },
  { name: 'MemoryContext', description: '记忆上下文摘要', resolver: 'memory.context' },
  { name: 'RecentHistory', description: '最近对话历史', resolver: 'memory.recent_history' },
]

function defaultBlocks(agentId: string): PromptBlock[] {
  return [
    {
      id: 'blk-identity',
      name: '身份设定',
      block_type: 'System',
      content: `你是 {{AgentName}}，一个专业的{{AgentRole}}。请始终以专业、准确的方式回答问题。`,
      variants: [
        `你是 {{AgentName}}，一个专业的{{AgentRole}}。请始终以专业、准确的方式回答问题。`,
        `作为 {{AgentName}}（{{AgentRole}}），你的目标是为用户提供最优质的帮助和建议。`,
      ],
      selected_variant: 0,
      enabled: true,
      order: 0,
    },
    {
      id: 'blk-memory',
      name: '记忆规则',
      block_type: 'Memory',
      content: `【记忆系统】以下是从长期记忆中检索的相关信息：\n{{MemoryContext}}\n\n请参考这些记忆来回答用户的问题，但不要直接引用记忆来源。`,
      variants: [
        `【记忆系统】以下是从长期记忆中检索的相关信息：\n{{MemoryContext}}\n\n请参考这些记忆来回答用户的问题，但不要直接引用记忆来源。`,
      ],
      selected_variant: 0,
      enabled: true,
      order: 1,
    },
    {
      id: 'blk-diary',
      name: '日记系统',
      block_type: 'Diary',
      content: `【日记】当前时间: {{DateTime}}\n你可以在回答中融入时间相关的上下文信息。`,
      variants: [
        `【日记】当前时间: {{DateTime}}\n你可以在回答中融入时间相关的上下文信息。`,
      ],
      selected_variant: 0,
      enabled: false,
      order: 2,
    },
    {
      id: 'blk-rules',
      name: '行为规范',
      block_type: 'Rules',
      content: `【规范】\n1. 使用 {{UserLanguage}} 语言回答\n2. 代码块使用 Markdown 格式\n3. 不确定时明确告知用户\n4. 保持回答简洁精确`,
      variants: [
        `【规范】\n1. 使用 {{UserLanguage}} 语言回答\n2. 代码块使用 Markdown 格式\n3. 不确定时明确告知用户\n4. 保持回答简洁精确`,
        `【规范】\n1. 回答须详尽完整\n2. 提供代码示例时附带注释\n3. 主动提出改进建议\n4. 使用结构化格式输出`,
      ],
      selected_variant: 0,
      enabled: true,
      order: 3,
    },
  ]
}

const _promptConfigCache = new Map<string, AgentPromptConfig>()

function getMockConfig(agentId: string): AgentPromptConfig {
  if (!_promptConfigCache.has(agentId)) {
    _promptConfigCache.set(agentId, {
      agent_id: agentId,
      mode: 'modular',
      original_prompt: `你是 {{AgentName}}，一个专业的{{AgentRole}}。\n\n当前时间: {{DateTime}}\n\n请以专业、准确的方式回答问题。`,
      blocks: defaultBlocks(agentId),
      updated_at: new Date().toISOString(),
    })
  }
  return _promptConfigCache.get(agentId)!
}

const MOCK_WAREHOUSES: PromptWarehouse[] = [
  {
    name: '全局仓库',
    is_global: true,
    blocks: [
      {
        id: 'wh-cot',
        name: '思维链',
        block_type: 'Reasoning',
        content: '请逐步分析问题，展示你的推理过程。在最终回答前，先列出关键思考步骤。',
        variants: ['请逐步分析问题，展示你的推理过程。在最终回答前，先列出关键思考步骤。'],
        selected_variant: 0,
        enabled: true,
        order: 0,
      },
      {
        id: 'wh-safety',
        name: '安全规范',
        block_type: 'Safety',
        content: '请勿生成有害、违法或不当的内容。始终遵循道德准则。',
        variants: ['请勿生成有害、违法或不当的内容。始终遵循道德准则。'],
        selected_variant: 0,
        enabled: true,
        order: 1,
      },
      {
        id: 'wh-format',
        name: '输出格式',
        block_type: 'Format',
        content: '输出时请使用 Markdown 格式，包括标题、列表和代码块。',
        variants: ['输出时请使用 Markdown 格式，包括标题、列表和代码块。'],
        selected_variant: 0,
        enabled: true,
        order: 2,
      },
    ],
  },
  {
    name: '私有仓库',
    is_global: false,
    blocks: [
      {
        id: 'wh-code-review',
        name: '代码审查',
        block_type: 'Task',
        content: '请对以下代码进行审查，重点关注：性能、安全性、可读性、最佳实践。',
        variants: ['请对以下代码进行审查，重点关注：性能、安全性、可读性、最佳实践。'],
        selected_variant: 0,
        enabled: true,
        order: 0,
      },
    ],
  },
]

export async function getPromptConfig(agentId: string): Promise<AgentPromptConfig> {
  // TODO: replace with real API call
  // if (IS_TAURI) return tauriInvoke('get_prompt_config', { agentId })
  // return httpGet(`/api/prompts/${agentId}`)
  await new Promise(r => setTimeout(r, 150))
  return JSON.parse(JSON.stringify(getMockConfig(agentId)))
}

export async function savePromptConfig(agentId: string, config: AgentPromptConfig): Promise<void> {
  // TODO: replace with real API call
  await new Promise(r => setTimeout(r, 100))
  _promptConfigCache.set(agentId, { ...config, updated_at: new Date().toISOString() })
}

export async function switchPromptMode(agentId: string, mode: PromptMode): Promise<void> {
  await new Promise(r => setTimeout(r, 50))
  const cfg = getMockConfig(agentId)
  cfg.mode = mode
  cfg.updated_at = new Date().toISOString()
}

export async function addBlock(agentId: string, block: PromptBlock): Promise<void> {
  await new Promise(r => setTimeout(r, 50))
  const cfg = getMockConfig(agentId)
  cfg.blocks.push(block)
  cfg.updated_at = new Date().toISOString()
}

export async function updateBlock(agentId: string, blockId: string, block: Partial<PromptBlock>): Promise<void> {
  await new Promise(r => setTimeout(r, 50))
  const cfg = getMockConfig(agentId)
  const target = cfg.blocks.find(b => b.id === blockId)
  if (target) Object.assign(target, block)
  cfg.updated_at = new Date().toISOString()
}

export async function deleteBlock(agentId: string, blockId: string): Promise<void> {
  await new Promise(r => setTimeout(r, 50))
  const cfg = getMockConfig(agentId)
  cfg.blocks = cfg.blocks.filter(b => b.id !== blockId)
  cfg.updated_at = new Date().toISOString()
}

export async function reorderBlocks(agentId: string, blockIds: string[]): Promise<void> {
  await new Promise(r => setTimeout(r, 50))
  const cfg = getMockConfig(agentId)
  const blockMap = new Map(cfg.blocks.map(b => [b.id, b]))
  cfg.blocks = blockIds.map((id, i) => {
    const b = blockMap.get(id)!
    b.order = i
    return b
  })
  cfg.updated_at = new Date().toISOString()
}

export async function addVariant(agentId: string, blockId: string, content: string): Promise<void> {
  await new Promise(r => setTimeout(r, 50))
  const cfg = getMockConfig(agentId)
  const block = cfg.blocks.find(b => b.id === blockId)
  if (block) block.variants.push(content)
  cfg.updated_at = new Date().toISOString()
}

export async function selectVariant(agentId: string, blockId: string, index: number): Promise<void> {
  await new Promise(r => setTimeout(r, 50))
  const cfg = getMockConfig(agentId)
  const block = cfg.blocks.find(b => b.id === blockId)
  if (block) {
    block.selected_variant = index
    block.content = block.variants[index] ?? block.content
  }
  cfg.updated_at = new Date().toISOString()
}

export async function listWarehouses(): Promise<PromptWarehouse[]> {
  await new Promise(r => setTimeout(r, 100))
  return JSON.parse(JSON.stringify(MOCK_WAREHOUSES))
}

export async function getWarehouse(name: string): Promise<PromptWarehouse> {
  await new Promise(r => setTimeout(r, 80))
  const wh = MOCK_WAREHOUSES.find(w => w.name === name)
  if (!wh) throw new Error(`Warehouse "${name}" not found`)
  return JSON.parse(JSON.stringify(wh))
}

export async function saveWarehouse(warehouse: PromptWarehouse): Promise<void> {
  await new Promise(r => setTimeout(r, 80))
  const idx = MOCK_WAREHOUSES.findIndex(w => w.name === warehouse.name)
  if (idx >= 0) MOCK_WAREHOUSES[idx] = warehouse
  else MOCK_WAREHOUSES.push(warehouse)
}

export async function deleteWarehouse(name: string): Promise<void> {
  await new Promise(r => setTimeout(r, 80))
  const idx = MOCK_WAREHOUSES.findIndex(w => w.name === name)
  if (idx >= 0) MOCK_WAREHOUSES.splice(idx, 1)
}

export async function listVariables(): Promise<PromptVariable[]> {
  await new Promise(r => setTimeout(r, 50))
  return [...MOCK_VARIABLES]
}

export async function previewPrompt(agentId: string): Promise<string> {
  await new Promise(r => setTimeout(r, 100))
  const cfg = getMockConfig(agentId)
  if (cfg.mode === 'original') {
    return cfg.original_prompt
  }
  if (cfg.mode === 'preset') {
    return cfg.preset_content ?? '(未选择预设)'
  }
  // modular: join enabled blocks
  return cfg.blocks
    .filter(b => b.enabled)
    .sort((a, b) => a.order - b.order)
    .map(b => `[${b.name}]\n${b.content}`)
    .join('\n\n')
}
