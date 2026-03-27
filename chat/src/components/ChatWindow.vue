<script setup lang="ts">
import { ref, watch, nextTick, computed } from 'vue'
import { sendChat } from '../api'
import type { ChatMessage, AgentInfo, ChatSession } from '../types'
import type { ChatMessagePayload } from '../api'
import MessageBubble from './MessageBubble.vue'
import ThinkingIndicator from './ThinkingIndicator.vue'
import EmptyState from './EmptyState.vue'

const props = defineProps<{
  agentId: string
  agent: AgentInfo | null
  session: ChatSession | null
  agentColor: string
}>()

const emit = defineEmits<{
  'update:session': [session: ChatSession]
  'new-message': []
}>()

const inputText = ref('')
const loading = ref(false)
const messagesContainer = ref<HTMLElement | null>(null)
const editingTitle = ref(false)
const titleInput = ref('')

const messages = computed(() => props.session?.messages ?? [])
const charCount = computed(() => inputText.value.length)

const canSend = computed(() => inputText.value.trim().length > 0 && !loading.value)

function startEditTitle() {
  if (!props.session) return
  titleInput.value = props.session.title
  editingTitle.value = true
}

function saveTitle() {
  if (!props.session) return
  editingTitle.value = false
  const updated = { ...props.session, title: titleInput.value || props.session.title }
  emit('update:session', updated)
}

async function handleSend(text?: string) {
  const msg = (text ?? inputText.value).trim()
  if (!msg || loading.value) return

  const userMsg: ChatMessage = {
    role: 'user',
    content: msg,
    timestamp: Date.now(),
  }

  const currentMessages = [...messages.value, userMsg]
  updateSessionMessages(currentMessages)
  inputText.value = ''
  loading.value = true

  await scrollToBottom()

  const history: ChatMessagePayload[] = currentMessages
    .filter((m) => m !== userMsg)
    .map((m) => ({ role: m.role, content: m.content }))

  try {
    const response = await sendChat(props.agentId, msg, history)
    const assistantMsg: ChatMessage = {
      role: 'assistant',
      content: response.message,
      sources: response.sources,
      latency_ms: response.latency_ms,
      timestamp: Date.now(),
    }
    updateSessionMessages([...currentMessages, assistantMsg])
  } catch (e) {
    const errorMsg: ChatMessage = {
      role: 'assistant',
      content: `请求失败: ${e instanceof Error ? e.message : '未知错误'}`,
      timestamp: Date.now(),
    }
    updateSessionMessages([...currentMessages, errorMsg])
  } finally {
    loading.value = false
    await scrollToBottom()
  }
}

function updateSessionMessages(msgs: ChatMessage[]) {
  if (!props.session) return
  const title = props.session.title === '新对话' && msgs.length > 0
    ? msgs[0].content.slice(0, 30)
    : props.session.title
  emit('update:session', {
    ...props.session,
    messages: msgs,
    title,
    updatedAt: Date.now(),
  })
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    handleSend()
  }
}

async function scrollToBottom() {
  await nextTick()
  if (messagesContainer.value) {
    messagesContainer.value.scrollTo({
      top: messagesContainer.value.scrollHeight,
      behavior: 'smooth',
    })
  }
}

function handlePrompt(text: string) {
  handleSend(text)
}

watch(() => props.session?.id, () => {
  nextTick(() => scrollToBottom())
})
</script>

<template>
  <div class="chat-window">
    <!-- Top bar -->
    <div class="top-bar">
      <div class="top-bar-left">
        <div class="top-agent-avatar" :style="{ borderColor: props.agentColor + '66' }">
          <span :style="{ color: props.agentColor }">
            {{ props.agent ? props.agent.name.charAt(0).toUpperCase() : '?' }}
          </span>
        </div>
        <div class="top-bar-info">
          <div class="top-agent-name" :style="{ color: props.agentColor }">
            {{ props.agent?.name || props.agentId }}
          </div>
          <div v-if="props.session" class="top-session-title" @click="startEditTitle">
            <input
              v-if="editingTitle"
              v-model="titleInput"
              class="title-edit-input"
              @blur="saveTitle"
              @keydown.enter="saveTitle"
            />
            <span v-else class="title-display">{{ props.session.title }}</span>
          </div>
        </div>
      </div>
      <div class="top-bar-right">
        <span class="memory-indicator">
          <span class="memory-dot"></span>
          记忆状态: {{ messages.length }} 条消息
        </span>
      </div>
    </div>

    <!-- Messages area -->
    <div ref="messagesContainer" class="messages-area">
      <EmptyState
        v-if="messages.length === 0"
        :agent="props.agent"
        @prompt="handlePrompt"
      />

      <MessageBubble
        v-for="(msg, idx) in messages"
        :key="idx"
        :message="msg"
        :agent-name="props.agent?.name"
        :agent-color="props.agentColor"
      />

      <ThinkingIndicator
        v-if="loading"
        :agent-name="props.agent?.name"
      />
    </div>

    <!-- Input area -->
    <div class="input-area">
      <div class="input-wrapper" :class="{ focused: false }">
        <textarea
          v-model="inputText"
          class="chat-input"
          :placeholder="`向 ${props.agent?.name || '智能体'} 提问...`"
          :disabled="loading"
          rows="1"
          @keydown="handleKeydown"
          @input="($event.target as HTMLTextAreaElement).style.height = 'auto'; ($event.target as HTMLTextAreaElement).style.height = Math.min(($event.target as HTMLTextAreaElement).scrollHeight, 150) + 'px'"
        ></textarea>
        <div class="input-footer">
          <span class="input-hints">Enter 发送 · Shift+Enter 换行</span>
          <span class="char-count" :class="{ active: charCount > 0 }">{{ charCount }}</span>
          <button
            class="send-btn"
            :disabled="!canSend"
            @click="handleSend()"
          >
            发送
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat-window {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #0a0e14;
}

/* Top bar */
.top-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  background: rgba(10, 14, 20, 0.95);
  backdrop-filter: blur(10px);
  flex-shrink: 0;
}

.top-bar-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.top-agent-avatar {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid;
  display: flex;
  align-items: center;
  justify-content: center;
  font-family: 'JetBrains Mono', monospace;
  font-size: 16px;
  font-weight: 700;
}

.top-bar-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.top-agent-name {
  font-family: 'JetBrains Mono', monospace;
  font-size: 14px;
  font-weight: 600;
}

.top-session-title {
  cursor: pointer;
}

.title-display {
  font-size: 12px;
  color: #6e7681;
  transition: color 0.2s;
}

.title-display:hover {
  color: #8b949e;
}

.title-edit-input {
  font-size: 12px;
  color: #e6edf3;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(0, 212, 255, 0.3);
  border-radius: 4px;
  padding: 2px 6px;
  outline: none;
  font-family: inherit;
}

.top-bar-right {
  display: flex;
  align-items: center;
}

.memory-indicator {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  color: #484f58;
  display: flex;
  align-items: center;
  gap: 6px;
}

.memory-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #00ff88;
  box-shadow: 0 0 6px rgba(0, 255, 136, 0.4);
  animation: dotPulse 2s ease-in-out infinite;
}

/* Messages */
.messages-area {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* Input area */
.input-area {
  padding: 16px 20px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  background: rgba(10, 14, 20, 0.95);
  flex-shrink: 0;
}

.input-wrapper {
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.03);
  transition: all 0.3s ease;
  overflow: hidden;
}

.input-wrapper:focus-within {
  border-color: rgba(0, 212, 255, 0.4);
  box-shadow: 0 0 12px rgba(0, 212, 255, 0.1);
}

.chat-input {
  width: 100%;
  padding: 12px 16px;
  background: transparent;
  border: none;
  outline: none;
  color: #e6edf3;
  font-size: 14px;
  font-family: inherit;
  line-height: 1.6;
  resize: none;
  min-height: 24px;
  max-height: 150px;
}

.chat-input::placeholder {
  color: #484f58;
}

.chat-input:disabled {
  opacity: 0.5;
}

.input-footer {
  display: flex;
  align-items: center;
  padding: 6px 12px 8px;
  gap: 10px;
}

.input-hints {
  font-size: 11px;
  color: #30363d;
  flex: 1;
}

.char-count {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  color: #30363d;
  transition: color 0.2s;
}

.char-count.active {
  color: #484f58;
}

.send-btn {
  padding: 6px 20px;
  border: none;
  border-radius: 8px;
  background: linear-gradient(135deg, #00d4ff, #7b2ff7);
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
  font-family: inherit;
}

.send-btn:hover:not(:disabled) {
  box-shadow: 0 0 16px rgba(0, 212, 255, 0.3);
  transform: translateY(-1px);
}

.send-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

@keyframes dotPulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

@media (max-width: 768px) {
  .top-bar-right {
    display: none;
  }
  .message-bubble {
    max-width: 88%;
  }
}
</style>
