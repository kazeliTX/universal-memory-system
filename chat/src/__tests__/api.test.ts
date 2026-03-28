import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const mockFetch = vi.fn()

beforeEach(() => {
  vi.stubGlobal('fetch', mockFetch)
})
afterEach(() => {
  vi.restoreAllMocks()
})

function jsonOk(body: unknown) {
  return {
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(JSON.stringify(body)),
  }
}

function jsonErr(status: number, body: string) {
  return {
    ok: false,
    status,
    json: () => Promise.resolve({}),
    text: () => Promise.resolve(body),
  }
}

// ---------------------------------------------------------------------------
// sendChat
// ---------------------------------------------------------------------------
describe('sendChat', () => {
  it('sends POST /api/chat with agent_id, message and history', async () => {
    const chatResp = {
      message: 'Hello!',
      agent_id: 'coder',
      sources: [],
      latency_ms: 120,
    }
    mockFetch.mockResolvedValueOnce(jsonOk(chatResp))

    const { sendChat } = await import('../api')
    const history = [{ role: 'user' as const, content: 'hi' }]
    const result = await sendChat('coder', 'hello', history)

    expect(mockFetch).toHaveBeenCalledWith('/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        agent_id: 'coder',
        message: 'hello',
        history,
      }),
    })
    expect(result).toEqual(chatResp)
  })

  it('throws on non-200 response', async () => {
    mockFetch.mockResolvedValueOnce(jsonErr(500, 'server broke'))

    const { sendChat } = await import('../api')
    await expect(sendChat('coder', 'hi', [])).rejects.toThrow('HTTP 500: server broke')
  })

  it('sends empty history array when no prior messages', async () => {
    mockFetch.mockResolvedValueOnce(jsonOk({ message: 'ok', agent_id: 'a', sources: [], latency_ms: 0 }))

    const { sendChat } = await import('../api')
    await sendChat('a', 'test', [])

    const lastCall = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]
    const body = JSON.parse(lastCall[1].body)
    expect(body.history).toEqual([])
  })
})

// ---------------------------------------------------------------------------
// listAgents
// ---------------------------------------------------------------------------
describe('listAgents', () => {
  it('sends GET /api/agents and returns agent list', async () => {
    const payload = {
      agents: [
        { agent_id: 'coder', name: 'Coder', role: 'dev', description: 'Codes', expertise: ['rust'] },
      ],
    }
    mockFetch.mockResolvedValueOnce(jsonOk(payload))

    const { listAgents } = await import('../api')
    const result = await listAgents()

    expect(mockFetch).toHaveBeenCalledWith('/api/agents')
    expect(result.agents).toHaveLength(1)
    expect(result.agents[0].agent_id).toBe('coder')
  })

  it('throws on non-200 response', async () => {
    mockFetch.mockResolvedValueOnce(jsonErr(503, ''))

    const { listAgents } = await import('../api')
    await expect(listAgents()).rejects.toThrow('HTTP 503')
  })
})
