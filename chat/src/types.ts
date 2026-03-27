export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  sources?: ChatSource[]
  latency_ms?: number
  timestamp: number
}

export interface ChatSource {
  content: string
  score: number
  memory_id: string
}

export interface ChatResponse {
  message: string
  agent_id: string
  sources: ChatSource[]
  latency_ms: number
}

export interface AgentInfo {
  agent_id: string
  name: string
  role: string
  description: string
  expertise: string[]
}

export interface ChatSession {
  id: string
  agentId: string
  title: string
  messages: ChatMessage[]
  createdAt: number
  updatedAt: number
}
