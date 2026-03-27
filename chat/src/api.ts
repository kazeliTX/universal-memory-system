import type { ChatResponse, AgentInfo } from './types'

const API_BASE = '/api'

export interface ChatMessagePayload {
  role: 'user' | 'assistant'
  content: string
}

export async function sendChat(
  agentId: string,
  message: string,
  history: ChatMessagePayload[],
): Promise<ChatResponse> {
  const res = await fetch(`${API_BASE}/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ agent_id: agentId, message, history }),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`)
  return res.json()
}

export async function listAgents(): Promise<{ agents: AgentInfo[] }> {
  const res = await fetch(`${API_BASE}/agents`)
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}
