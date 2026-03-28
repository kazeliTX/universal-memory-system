<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { NConfigProvider, darkTheme } from 'naive-ui'
import { listAgents } from './api'
import AgentSelector from './components/AgentSelector.vue'
import ChatWindow from './components/ChatWindow.vue'
import { useEvents } from './composables/useEvents'
import type { AgentInfo, ChatSession } from './types'

const { events, connected } = useEvents()

const selectedAgent = ref('coder')
const sessions = ref<ChatSession[]>([])
const currentSessionId = ref<string | null>(null)
const sidebarOpen = ref(true)
const agents = ref<AgentInfo[]>([])

const STORAGE_KEY = 'umms-chat-sessions'

const agentColors: Record<string, string> = {
  coder: '#00d4ff',
  researcher: '#7b2ff7',
  writer: '#ff006e',
  analyst: '#00ff88',
  default: '#00d4ff',
}

function getAgentColor(agentId: string): string {
  return agentColors[agentId] || agentColors.default
}

const currentAgentInfo = computed(() => {
  return agents.value.find((a) => a.agent_id === selectedAgent.value) ?? null
})

// Load sessions from localStorage
function loadSessions() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      sessions.value = JSON.parse(raw)
    }
  } catch (_e) {
    sessions.value = []
  }
}

// Save sessions to localStorage
function saveSessions() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(sessions.value))
  } catch (_e) {
    // Storage full or unavailable
  }
}

function generateId(): string {
  return crypto.randomUUID()
}

function createNewSession() {
  const session: ChatSession = {
    id: generateId(),
    agentId: selectedAgent.value,
    title: '新对话',
    messages: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  sessions.value.push(session)
  currentSessionId.value = session.id
  saveSessions()
}

function selectSession(sessionId: string) {
  const session = sessions.value.find((s) => s.id === sessionId)
  if (session) {
    selectedAgent.value = session.agentId
    currentSessionId.value = sessionId
  }
}

function deleteSession(sessionId: string) {
  sessions.value = sessions.value.filter((s) => s.id !== sessionId)
  if (currentSessionId.value === sessionId) {
    const agentSessions = sessions.value.filter((s) => s.agentId === selectedAgent.value)
    currentSessionId.value = agentSessions.length > 0 ? agentSessions[0].id : null
  }
  saveSessions()
}

function handleUpdateSession(updated: ChatSession) {
  const idx = sessions.value.findIndex((s) => s.id === updated.id)
  if (idx >= 0) {
    sessions.value[idx] = updated
    saveSessions()
  }
}

const currentSession = computed(() => {
  if (!currentSessionId.value) return null
  return sessions.value.find((s) => s.id === currentSessionId.value) ?? null
})

// When agent changes, pick the latest session for that agent or create one
watch(selectedAgent, (agentId) => {
  const agentSessions = sessions.value
    .filter((s) => s.agentId === agentId)
    .sort((a, b) => b.updatedAt - a.updatedAt)
  if (agentSessions.length > 0) {
    currentSessionId.value = agentSessions[0].id
  } else {
    createNewSession()
  }
})

function toggleSidebar() {
  sidebarOpen.value = !sidebarOpen.value
}

onMounted(async () => {
  // Load agents
  try {
    const data = await listAgents()
    agents.value = data.agents
  } catch (_e) {
    agents.value = [
      { agent_id: 'coder', name: 'Coder', role: '开发工程师', description: '编程助手', expertise: [] },
    ]
  }

  loadSessions()
  // Ensure we have a session for the default agent
  const agentSessions = sessions.value.filter((s) => s.agentId === selectedAgent.value)
  if (agentSessions.length > 0) {
    currentSessionId.value = agentSessions.sort((a, b) => b.updatedAt - a.updatedAt)[0].id
  } else {
    createNewSession()
  }
})
</script>

<template>
  <NConfigProvider :theme="darkTheme">
    <div class="app-layout">
      <!-- Scan lines overlay -->
      <div class="scanlines"></div>

      <!-- Mobile sidebar toggle -->
      <button class="sidebar-toggle" @click="toggleSidebar">
        <span v-if="sidebarOpen">◀</span>
        <span v-else>▶</span>
      </button>

      <!-- Sidebar -->
      <aside class="app-sidebar" :class="{ collapsed: !sidebarOpen }">
        <AgentSelector
          v-model:agent="selectedAgent"
          :agents="agents"
          :sessions="sessions"
          :current-session-id="currentSessionId"
          @select-session="selectSession"
          @new-session="createNewSession"
          @delete-session="deleteSession"
        />
      </aside>

      <!-- WebSocket connection indicator -->
      <div class="ws-indicator" :title="connected ? 'WebSocket 已连接' : 'WebSocket 未连接'">
        <span class="ws-dot" :class="{ connected }"></span>
        <span v-if="events.length > 0" class="ws-badge">{{ events.length }}</span>
      </div>

      <!-- Main chat area -->
      <main class="main-area">
        <ChatWindow
          :agent-id="selectedAgent"
          :agent="currentAgentInfo"
          :session="currentSession"
          :agent-color="getAgentColor(selectedAgent)"
          @update:session="handleUpdateSession"
        />
      </main>
    </div>
  </NConfigProvider>
</template>

<style>
/* Global grid background */
.app-layout {
  display: flex;
  height: 100vh;
  overflow: hidden;
  background:
    linear-gradient(rgba(0, 212, 255, 0.02) 1px, transparent 1px),
    linear-gradient(90deg, rgba(0, 212, 255, 0.02) 1px, transparent 1px),
    #0a0e14;
  background-size: 40px 40px, 40px 40px;
  position: relative;
}

.scanlines {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  z-index: 1000;
  background: repeating-linear-gradient(
    0deg,
    transparent,
    transparent 2px,
    rgba(0, 0, 0, 0.03) 2px,
    rgba(0, 0, 0, 0.03) 4px
  );
}

.app-sidebar {
  width: 280px;
  flex-shrink: 0;
  border-right: 1px solid rgba(255, 255, 255, 0.04);
  transition: all 0.3s ease;
  z-index: 10;
}

.app-sidebar.collapsed {
  width: 0;
  overflow: hidden;
  border-right: none;
}

.main-area {
  flex: 1;
  min-width: 0;
  z-index: 1;
}

.sidebar-toggle {
  display: none;
  position: fixed;
  top: 12px;
  left: 12px;
  z-index: 100;
  background: rgba(0, 212, 255, 0.1);
  border: 1px solid rgba(0, 212, 255, 0.3);
  color: #00d4ff;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 12px;
  align-items: center;
  justify-content: center;
}

.ws-indicator {
  position: fixed;
  top: 12px;
  right: 16px;
  z-index: 100;
  display: flex;
  align-items: center;
  gap: 6px;
}

.ws-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #555;
  transition: background 0.3s ease;
}

.ws-dot.connected {
  background: #00ff88;
  box-shadow: 0 0 6px rgba(0, 255, 136, 0.5);
}

.ws-badge {
  font-size: 10px;
  color: #00d4ff;
  background: rgba(0, 212, 255, 0.1);
  border: 1px solid rgba(0, 212, 255, 0.3);
  border-radius: 10px;
  padding: 0 6px;
  line-height: 16px;
}

@media (max-width: 768px) {
  .sidebar-toggle {
    display: flex;
  }

  .app-sidebar {
    position: fixed;
    top: 0;
    left: 0;
    height: 100vh;
    z-index: 50;
    background: #0c1018;
  }

  .app-sidebar.collapsed {
    transform: translateX(-100%);
    width: 280px;
  }
}
</style>
