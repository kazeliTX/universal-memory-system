<script setup lang="ts">
import type { AgentInfo } from '../types'

const props = defineProps<{
  agent: AgentInfo | null
}>()

const emit = defineEmits<{
  prompt: [text: string]
}>()

function getDefaultPrompts(agent: AgentInfo | null): string[] {
  if (!agent) return []
  const expertiseStr = agent.expertise.join('、') || agent.role
  return [
    `请介绍一下你擅长的领域`,
    `帮我解决一个关于${agent.expertise[0] || agent.role}的问题`,
    `你能做哪些事情？`,
    `给我一些关于${expertiseStr}的建议`,
  ]
}

function avatarChar(name: string): string {
  return name.charAt(0).toUpperCase()
}

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
</script>

<template>
  <div class="empty-state">
    <div v-if="props.agent" class="empty-content">
      <div
        class="empty-avatar"
        :style="{
          background: `linear-gradient(135deg, ${getAgentColor(props.agent.agent_id)}33, ${getAgentColor(props.agent.agent_id)}11)`,
          borderColor: getAgentColor(props.agent.agent_id) + '66',
          boxShadow: `0 0 30px ${getAgentColor(props.agent.agent_id)}22`,
        }"
      >
        <span
          class="avatar-char"
          :style="{ color: getAgentColor(props.agent.agent_id) }"
        >
          {{ avatarChar(props.agent.name) }}
        </span>
      </div>

      <h2 class="empty-greeting">
        你好！我是 <span class="agent-name" :style="{ color: getAgentColor(props.agent.agent_id) }">{{ props.agent.name }}</span>
      </h2>
      <p class="empty-role">{{ props.agent.role }}</p>
      <p class="empty-hint">
        你可以向我提问任何关于
        <span class="highlight">{{ props.agent.expertise.join('、') || props.agent.role }}</span>
        的问题
      </p>

      <div class="prompt-cards">
        <button
          v-for="(prompt, idx) in getDefaultPrompts(props.agent)"
          :key="idx"
          class="prompt-card"
          :style="{
            '--accent': getAgentColor(props.agent.agent_id),
          }"
          @click="emit('prompt', prompt)"
        >
          <span class="prompt-icon">💡</span>
          <span class="prompt-text">{{ prompt }}</span>
        </button>
      </div>
    </div>

    <div v-else class="empty-content">
      <div class="empty-logo">UMMS</div>
      <p class="empty-role">Universal Memory System</p>
      <p class="empty-hint">请选择一个智能体开始对话</p>
    </div>
  </div>
</template>

<style scoped>
.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: 40px 20px;
}

.empty-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  max-width: 480px;
  animation: fadeIn 0.5s ease;
}

.empty-avatar {
  width: 80px;
  height: 80px;
  border-radius: 20px;
  border: 2px solid;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 20px;
}

.avatar-char {
  font-size: 36px;
  font-weight: 700;
  font-family: 'JetBrains Mono', monospace;
}

.empty-greeting {
  font-size: 22px;
  font-weight: 600;
  color: #e6edf3;
  margin-bottom: 6px;
}

.agent-name {
  font-family: 'JetBrains Mono', monospace;
}

.empty-role {
  font-size: 14px;
  color: #8b949e;
  margin-bottom: 8px;
}

.empty-hint {
  font-size: 14px;
  color: #6e7681;
  margin-bottom: 28px;
}

.highlight {
  color: #c9d1d9;
  font-weight: 500;
}

.empty-logo {
  font-family: 'JetBrains Mono', monospace;
  font-size: 48px;
  font-weight: 700;
  color: #00d4ff;
  text-shadow: 0 0 20px rgba(0, 212, 255, 0.3);
  margin-bottom: 8px;
}

.prompt-cards {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
}

.prompt-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
  text-align: left;
  color: #c9d1d9;
  font-size: 13px;
}

.prompt-card:hover {
  background: rgba(var(--accent-rgb, 0, 212, 255), 0.08);
  border-color: var(--accent, #00d4ff);
  box-shadow: 0 0 12px rgba(0, 212, 255, 0.15);
  transform: translateX(4px);
}

.prompt-icon {
  font-size: 16px;
  flex-shrink: 0;
}

.prompt-text {
  flex: 1;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(16px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>
