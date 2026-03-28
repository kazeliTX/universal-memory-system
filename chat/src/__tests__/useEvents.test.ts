import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { nextTick } from 'vue'
import { withSetup } from './test-utils'

// ---------------------------------------------------------------------------
// WebSocket mock
// ---------------------------------------------------------------------------

class MockWebSocket {
  static instances: MockWebSocket[] = []

  onopen: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
  url: string
  closed = false

  constructor(url: string) {
    this.url = url
    MockWebSocket.instances.push(this)
  }

  close() {
    this.closed = true
  }

  simulateOpen() {
    this.onopen?.()
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) })
  }

  simulateClose() {
    this.onclose?.()
  }

  simulateError() {
    this.onerror?.()
  }
}

beforeEach(() => {
  MockWebSocket.instances = []
  vi.stubGlobal('WebSocket', MockWebSocket)
  vi.useFakeTimers()
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.useRealTimers()
})

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('useEvents', () => {
  it('connects to WebSocket on mount and sets connected=true', async () => {
    const { useEvents } = await import('../composables/useEvents')
    const [result, app] = withSetup(() => useEvents())

    expect(MockWebSocket.instances).toHaveLength(1)
    expect(MockWebSocket.instances[0].url).toContain('/ws/events')
    expect(result.connected.value).toBe(false)

    MockWebSocket.instances[0].simulateOpen()
    expect(result.connected.value).toBe(true)

    app.unmount()
  })

  it('parses incoming JSON events into reactive array', async () => {
    const { useEvents } = await import('../composables/useEvents')
    const [result, app] = withSetup(() => useEvents())

    const ws = MockWebSocket.instances[0]
    ws.simulateOpen()

    const event = {
      id: 1,
      timestamp: '2026-03-28T10:00:00Z',
      event_type: 'memory_created',
      agent_id: 'coder',
      details: {},
    }
    ws.simulateMessage(event)

    expect(result.events.value).toHaveLength(1)
    expect(result.events.value[0].event_type).toBe('memory_created')
    expect(result.events.value[0].agent_id).toBe('coder')

    app.unmount()
  })

  it('keeps max 100 events (FIFO — newest first)', async () => {
    const { useEvents } = await import('../composables/useEvents')
    const [result, app] = withSetup(() => useEvents())

    const ws = MockWebSocket.instances[0]
    ws.simulateOpen()

    for (let i = 0; i < 110; i++) {
      ws.simulateMessage({
        id: i,
        timestamp: '2026-03-28T10:00:00Z',
        event_type: 'test',
        agent_id: 'coder',
        details: { seq: i },
      })
    }

    expect(result.events.value).toHaveLength(100)
    // Newest event should be at index 0
    expect(result.events.value[0].id).toBe(109)

    app.unmount()
  })

  it('silently ignores malformed JSON messages', async () => {
    const { useEvents } = await import('../composables/useEvents')
    const [result, app] = withSetup(() => useEvents())

    const ws = MockWebSocket.instances[0]
    ws.simulateOpen()

    // Send invalid JSON directly
    ws.onmessage?.({ data: 'not valid json {{' })

    expect(result.events.value).toHaveLength(0)

    app.unmount()
  })

  it('sets connected=false on close and auto-reconnects after 3s', async () => {
    const { useEvents } = await import('../composables/useEvents')
    const [result, app] = withSetup(() => useEvents())

    const ws = MockWebSocket.instances[0]
    ws.simulateOpen()
    expect(result.connected.value).toBe(true)

    ws.simulateClose()
    expect(result.connected.value).toBe(false)
    expect(MockWebSocket.instances).toHaveLength(1)

    // After 3 seconds, a new WebSocket should be created
    vi.advanceTimersByTime(3000)
    expect(MockWebSocket.instances).toHaveLength(2)

    app.unmount()
  })

  it('disconnects cleanly on unmount (no reconnect)', async () => {
    const { useEvents } = await import('../composables/useEvents')
    const [result, app] = withSetup(() => useEvents())

    const ws = MockWebSocket.instances[0]
    ws.simulateOpen()

    app.unmount()
    expect(ws.closed).toBe(true)

    // No reconnect should happen
    vi.advanceTimersByTime(5000)
    expect(MockWebSocket.instances).toHaveLength(1)
  })

  it('closes WebSocket on error event', async () => {
    const { useEvents } = await import('../composables/useEvents')
    const [result, app] = withSetup(() => useEvents())

    const ws = MockWebSocket.instances[0]
    ws.simulateOpen()

    ws.simulateError()
    expect(ws.closed).toBe(true)

    app.unmount()
  })
})
