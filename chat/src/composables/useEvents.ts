import { ref, onMounted, onUnmounted } from 'vue'

export interface AuditEvent {
  id: number
  timestamp: string
  event_type: string
  agent_id: string
  memory_id?: string
  node_id?: string
  layer?: string
  details: Record<string, unknown>
}

export function useEvents() {
  const events = ref<AuditEvent[]>([])
  const connected = ref(false)
  let ws: WebSocket | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null

  function connect() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const url = `${protocol}//${window.location.host}/ws/events`

    ws = new WebSocket(url)

    ws.onopen = () => {
      connected.value = true
      if (reconnectTimer) {
        clearTimeout(reconnectTimer)
        reconnectTimer = null
      }
    }

    ws.onmessage = (e) => {
      try {
        const event: AuditEvent = JSON.parse(e.data)
        events.value.unshift(event)
        // Keep only last 100 events
        if (events.value.length > 100) {
          events.value.length = 100
        }
      } catch {
        // ignore malformed messages
      }
    }

    ws.onclose = () => {
      connected.value = false
      ws = null
      // Auto-reconnect after 3 seconds
      reconnectTimer = setTimeout(connect, 3000)
    }

    ws.onerror = () => {
      ws?.close()
    }
  }

  function disconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
    ws?.close()
    ws = null
    connected.value = false
  }

  onMounted(connect)
  onUnmounted(disconnect)

  return { events, connected }
}
