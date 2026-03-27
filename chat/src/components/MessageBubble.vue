<script setup lang="ts">
import { ref } from 'vue'
import type { ChatMessage } from '../types'
import SourcePanel from './SourcePanel.vue'

const props = defineProps<{
  message: ChatMessage
  agentName?: string
  agentColor?: string
}>()

const showSources = ref(false)
const showTime = ref(false)

function formatTime(ts: number): string {
  const d = new Date(ts)
  return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}
</script>

<template>
  <div
    class="message-row"
    :class="{ 'message-user': props.message.role === 'user', 'message-assistant': props.message.role === 'assistant' }"
    @mouseenter="showTime = true"
    @mouseleave="showTime = false"
  >
    <!-- Agent avatar for assistant messages -->
    <div v-if="props.message.role === 'assistant'" class="msg-avatar" :style="{ borderColor: (props.agentColor || '#ff006e') + '66' }">
      <span :style="{ color: props.agentColor || '#ff006e' }">{{ (props.agentName || 'A').charAt(0).toUpperCase() }}</span>
    </div>

    <div class="message-bubble" :class="props.message.role">
      <!-- Content -->
      <div class="message-content">
        {{ props.message.content }}
      </div>

      <!-- Footer -->
      <div class="message-footer">
        <transition name="fade">
          <span v-if="showTime" class="msg-time">
            {{ formatTime(props.message.timestamp) }}
          </span>
        </transition>

        <span v-if="props.message.latency_ms" class="msg-latency">
          {{ props.message.latency_ms }}ms
        </span>

        <button
          v-if="props.message.sources && props.message.sources.length > 0"
          class="sources-toggle"
          @click="showSources = !showSources"
        >
          {{ showSources ? '收起记忆' : '查看关联记忆' }} ({{ props.message.sources.length }}条)
        </button>
      </div>

      <!-- Source panel -->
      <SourcePanel
        v-if="showSources && props.message.sources && props.message.sources.length > 0"
        :sources="props.message.sources"
      />
    </div>
  </div>
</template>

<style scoped>
.message-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  animation: messageIn 0.3s ease;
}

.message-user {
  justify-content: flex-end;
}

.message-assistant {
  justify-content: flex-start;
}

.msg-avatar {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: rgba(255, 0, 110, 0.08);
  border: 1px solid;
  display: flex;
  align-items: center;
  justify-content: center;
  font-family: 'JetBrains Mono', monospace;
  font-size: 14px;
  font-weight: 700;
  flex-shrink: 0;
  margin-top: 2px;
}

.message-bubble {
  max-width: 72%;
  padding: 12px 16px;
  position: relative;
}

.message-bubble.user {
  background: rgba(0, 212, 255, 0.08);
  border-left: 3px solid #00d4ff;
  border-radius: 12px 4px 12px 12px;
}

.message-bubble.assistant {
  background: rgba(255, 0, 110, 0.05);
  border-left: 3px solid #ff006e;
  border-radius: 4px 12px 12px 12px;
}

.message-content {
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.7;
  font-size: 14px;
  color: #e6edf3;
}

.message-footer {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
  min-height: 18px;
  flex-wrap: wrap;
}

.msg-time {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  color: #484f58;
}

.msg-latency {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  color: #484f58;
  background: rgba(255, 255, 255, 0.04);
  padding: 1px 6px;
  border-radius: 4px;
}

.sources-toggle {
  font-size: 12px;
  color: #7b2ff7;
  cursor: pointer;
  background: none;
  border: none;
  padding: 2px 0;
  font-family: inherit;
  transition: color 0.2s;
}

.sources-toggle:hover {
  color: #a855f7;
}

.fade-enter-active, .fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from, .fade-leave-to {
  opacity: 0;
}

@keyframes messageIn {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
</style>
