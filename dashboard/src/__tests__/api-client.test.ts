import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// The client detects Tauri via `'__TAURI_INTERNALS__' in window`.
// In jsdom there is no such property, so all calls go through fetch — which is what we test.

const mockFetch = vi.fn()

beforeEach(() => {
  vi.stubGlobal('fetch', mockFetch)
})
afterEach(() => {
  vi.restoreAllMocks()
})

// Helper: simulate a successful JSON response
function jsonOk(body: unknown) {
  return {
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(JSON.stringify(body)),
  }
}

// Helper: simulate an error response
function jsonErr(status: number, body: string) {
  return {
    ok: false,
    status,
    json: () => Promise.resolve({}),
    text: () => Promise.resolve(body),
  }
}

// ---------------------------------------------------------------------------
// getHealth
// ---------------------------------------------------------------------------
describe('getHealth', () => {
  it('calls GET /api/health and returns the parsed body', async () => {
    const payload = { status: 'ok', uptime_secs: 42 }
    mockFetch.mockResolvedValueOnce(jsonOk(payload))

    const { getHealth } = await import('@/api/client')
    const result = await getHealth()

    expect(mockFetch).toHaveBeenCalledWith('/api/health')
    expect(result).toEqual(payload)
  })

  it('throws on non-200 response', async () => {
    mockFetch.mockResolvedValueOnce(jsonErr(500, 'internal error'))

    const { getHealth } = await import('@/api/client')
    await expect(getHealth()).rejects.toThrow('HTTP 500: internal error')
  })
})

// ---------------------------------------------------------------------------
// getStats
// ---------------------------------------------------------------------------
describe('getStats', () => {
  it('calls GET /api/stats', async () => {
    const payload = { memory_count: 100, agent_count: 2 }
    mockFetch.mockResolvedValueOnce(jsonOk(payload))

    const { getStats } = await import('@/api/client')
    const result = await getStats()

    expect(mockFetch).toHaveBeenCalledWith('/api/stats')
    expect(result).toEqual(payload)
  })
})

// ---------------------------------------------------------------------------
// semanticSearch
// ---------------------------------------------------------------------------
describe('semanticSearch', () => {
  it('sends POST /api/search with correct body', async () => {
    const payload = { results: [{ content: 'hello', score: 0.9 }] }
    mockFetch.mockResolvedValueOnce(jsonOk(payload))

    const { semanticSearch } = await import('@/api/client')
    const result = await semanticSearch('test query', 'agent-1', 10, false)

    expect(mockFetch).toHaveBeenCalledWith('/api/search', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: 'test query',
        agent_id: 'agent-1',
        top_k: 10,
        include_shared: false,
      }),
    })
    expect(result).toEqual(payload)
  })

  it('uses default params when optional args omitted', async () => {
    mockFetch.mockResolvedValueOnce(jsonOk({ results: [] }))

    const { semanticSearch } = await import('@/api/client')
    await semanticSearch('q')

    const lastCall = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]
    const body = JSON.parse(lastCall[1].body)
    expect(body.top_k).toBe(5)
    expect(body.include_shared).toBe(true)
    expect(body.agent_id).toBeUndefined()
  })

  it('throws on error response', async () => {
    mockFetch.mockResolvedValueOnce(jsonErr(422, 'bad query'))

    const { semanticSearch } = await import('@/api/client')
    await expect(semanticSearch('q')).rejects.toThrow('HTTP 422: bad query')
  })
})

// ---------------------------------------------------------------------------
// createAgent
// ---------------------------------------------------------------------------
describe('createAgent', () => {
  it('sends POST /api/agents with the request body', async () => {
    const req = {
      agent_id: 'writer',
      name: 'Writer',
      role: 'content',
      description: 'Writes things',
      expertise: ['writing'],
    }
    const resp = { agent_id: 'writer', name: 'Writer' }
    mockFetch.mockResolvedValueOnce(jsonOk(resp))

    const { createAgent } = await import('@/api/client')
    const result = await createAgent(req)

    expect(mockFetch).toHaveBeenCalledWith('/api/agents', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    })
    expect(result).toEqual(resp)
  })
})

// ---------------------------------------------------------------------------
// deleteAgent
// ---------------------------------------------------------------------------
describe('deleteAgent', () => {
  it('sends DELETE /api/agents/:id', async () => {
    const resp = { deleted: true }
    mockFetch.mockResolvedValueOnce(jsonOk(resp))

    const { deleteAgent } = await import('@/api/client')
    const result = await deleteAgent('old-agent')

    expect(mockFetch).toHaveBeenCalledWith('/api/agents/old-agent', { method: 'DELETE' })
    expect(result).toEqual(resp)
  })
})

// ---------------------------------------------------------------------------
// getMetrics (returns text, not JSON)
// ---------------------------------------------------------------------------
describe('getMetrics', () => {
  it('calls GET /api/metrics and returns raw text', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      text: () => Promise.resolve('# HELP umms_uptime\numms_uptime 42'),
    })

    const { getMetrics } = await import('@/api/client')
    const result = await getMetrics()

    expect(mockFetch).toHaveBeenCalledWith('/api/metrics')
    expect(result).toContain('umms_uptime 42')
  })
})
