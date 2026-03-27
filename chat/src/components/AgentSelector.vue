<script setup lang="ts">
import { computed } from 'vue'
import type { AgentInfo, ChatSession } from '../types'

const props = defineProps<{
  agent: string
  agents: AgentInfo[]
  sessions: ChatSession[]
  currentSessionId: string | null
}>()

const emit = defineEmits<{
  'update:agent': [value: string]
  'select-session': [sessionId: string]
  'new-session': []
  'delete-session': [sessionId: string]
}>()

const agentColors: Record<string, string> = {
  coder: '#00d4ff',
  researcher: '#7b2ff7',
  writer: '#ff006e',
  analyst: '#00ff88',
  default: '#00d4ff',
}

const agentGradients: Record<string, string> = {
  coder: 'linear-gradient(135deg, #00d4ff, #0088cc)',
  researcher: 'linear-gradient(135deg, #7b2ff7, #5500cc)',
  writer: 'linear-gradient(135deg, #ff006e, #cc0055)',
  analyst: 'linear-gradient(135deg, #00ff88, #00cc66)',
  default: 'linear-gradient(135deg, #00d4ff, #7b2ff7)',
}

function getColor(id: string): string {
  return agentColors[id] || agentColors.default
}

function getGradient(id: string): string {
  return agentGradients[id] || agentGradients.default
}

function selectAgent(id: string) {
  emit('update:agent', id)
}

function avatarChar(name: string): string {
  return name.charAt(0).toUpperCase()
}

const agentSessions = computed(() => {
  return props.sessions
    .filter((s) => s.agentId === props.agent)
    .sort((a, b) => b.updatedAt - a.updatedAt)
})

function formatRelativeTime(ts: number): string {
  const diff = Date.now() - ts
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return '刚刚'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}分钟前`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}小时前`
  const days = Math.floor(hours / 24)
  if (days === 1) return '昨天'
  if (days < 7) return `${days}天前`
  return new Date(ts).toLocaleDateString('zh-CN')
}

function truncatePreview(text: string): string {
  if (text.length <= 30) return text
  return text.slice(0, 30) + '...'
}

function getLastMessage(session: ChatSession): string {
  if (session.messages.length === 0) return '暂无消息'
  return session.messages[session.messages.length - 1].content
}

function handleDeleteSession(e: Event, sessionId: string) {
  e.stopPropagation()
  emit('delete-session', sessionId)
}
</script>

<template>
  <div class="sidebar">
    <!-- Logo -->
    <div class="logo-section">
      <div class="logo-text">UMMS</div>
      <div class="logo-sub">Universal Memory System</div>
    </div>

    <!-- Agent cards -->
    <div class="section-label">智能体</div>
    <div class="agents-list">
      <div
        v-for="a in props.agents"
        :key="a.agent_id"
        class="agent-card"
        :class="{ active: a.agent_id === props.agent }"
        @click="selectAgent(a.agent_id)"
      >
        <div class="agent-avatar" :style="{ background: getGradient(a.agent_id) }">
          {{ avatarChar(a.name) }}
        </div>
        <div class="agent-info">
          <div class="agent-name">{{ a.name }}</div>
          <div class="agent-role">{{ a.role }}</div>
        </div>
        <div v-if="a.expertise && a.expertise.length > 0" class="agent-tags">
          <span
            v-for="tag in a.expertise.slice(0, 3)"
            :key="tag"
            class="tag-pill"
            :style="{ color: getColor(a.agent_id), borderColor: getColor(a.agent_id) + '44' }"
          >
            {{ tag }}
          </span>
        </div>
      </div>
    </div>

    <!-- New session button -->
    <button class="new-session-btn" @click="emit('new-session')">
      <span class="plus-icon">+</span>
      新建对话
    </button>

    <!-- Sessions list -->
    <div class="section-label">对话记录</div>
    <div class="sessions-list">
      <div v-if="agentSessions.length === 0" class="empty-sessions">
        暂无对话记录
      </div>
      <div
        v-for="s in agentSessions"
        :key="s.id"
        class="session-item"
        :class="{ active: s.id === props.currentSessionId }"
        @click="emit('select-session', s.id)"
      >
        <div class="session-main">
          <div class="session-title">{{ s.title }}</div>
          <div class="session-preview">{{ truncatePreview(getLastMessage(s)) }}</div>
        </div>
        <div class="session-meta">
          <span class="session-time">{{ formatRelativeTime(s.updatedAt) }}</span>
          <button
            class="session-delete"
            title="删除对话"
            @click="(e: Event) => handleDeleteSession(e, s.id)"
          >
            ×
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  height: 100vh;
  padding: 0;
  overflow: hidden;
  background: #0c1018;
}

/* Logo */
.logo-section {
  padding: 20px 16px 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
}

.logo-text {
  font-family: 'JetBrains Mono', monospace;
  font-size: 24px;
  font-weight: 700;
  color: #00d4ff;
  text-shadow: 0 0 15px rgba(0, 212, 255, 0.3);
  letter-spacing: 2px;
}

.logo-sub {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  color: #484f58;
  margin-top: 2px;
  letter-spacing: 0.5px;
}

.section-label {
  font-size: 11px;
  color: #484f58;
  padding: 12px 16px 6px;
  font-family: 'JetBrains Mono', monospace;
  text-transform: uppercase;
  letter-spacing: 1px;
}

/* Agents */
.agents-list {
  padding: 0 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.agent-card {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  padding: 10px;
  border-radius: 10px;
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.2s ease;
  background: rgba(255, 255, 255, 0.02);
}

.agent-card:hover {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.06);
}

.agent-card.active {
  background: rgba(0, 212, 255, 0.06);
  border-color: rgba(0, 212, 255, 0.25);
  box-shadow: 0 0 12px rgba(0, 212, 255, 0.08);
}

.agent-avatar {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-family: 'JetBrains Mono', monospace;
  font-size: 18px;
  font-weight: 700;
  color: #fff;
  flex-shrink: 0;
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}

.agent-info {
  flex: 1;
  min-width: 0;
}

.agent-name {
  font-size: 14px;
  font-weight: 600;
  color: #e6edf3;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.agent-role {
  font-size: 11px;
  color: #6e7681;
  margin-top: 1px;
}

.agent-tags {
  width: 100%;
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
  padding-left: 50px;
}

.tag-pill {
  font-size: 10px;
  padding: 1px 8px;
  border: 1px solid;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.02);
}

/* New session */
.new-session-btn {
  margin: 10px 10px 0;
  padding: 10px;
  border-radius: 10px;
  border: 1px dashed rgba(0, 212, 255, 0.3);
  background: transparent;
  color: #00d4ff;
  font-size: 13px;
  font-family: inherit;
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
}

.new-session-btn:hover {
  background: rgba(0, 212, 255, 0.06);
  border-color: rgba(0, 212, 255, 0.5);
  box-shadow: 0 0 10px rgba(0, 212, 255, 0.1);
}

.plus-icon {
  font-size: 16px;
  font-weight: 300;
}

/* Sessions */
.sessions-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 10px 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.empty-sessions {
  font-size: 12px;
  color: #30363d;
  padding: 12px 8px;
  text-align: center;
}

.session-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 10px;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.15s ease;
  border: 1px solid transparent;
}

.session-item:hover {
  background: rgba(255, 255, 255, 0.03);
}

.session-item.active {
  background: rgba(0, 212, 255, 0.05);
  border-color: rgba(0, 212, 255, 0.15);
}

.session-main {
  flex: 1;
  min-width: 0;
}

.session-title {
  font-size: 13px;
  color: #c9d1d9;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.session-preview {
  font-size: 11px;
  color: #484f58;
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.session-meta {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
  flex-shrink: 0;
  margin-left: 8px;
}

.session-time {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  color: #30363d;
  white-space: nowrap;
}

.session-delete {
  background: none;
  border: none;
  color: #30363d;
  font-size: 14px;
  cursor: pointer;
  padding: 0 2px;
  line-height: 1;
  transition: color 0.2s;
  display: none;
  font-family: inherit;
}

.session-item:hover .session-delete {
  display: block;
}

.session-delete:hover {
  color: #ff006e;
}
</style>
