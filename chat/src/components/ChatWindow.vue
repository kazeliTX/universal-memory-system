<script setup lang="ts">
import { ref, watch, nextTick, computed } from 'vue'
import { NInput, NButton, NSpace, NSpin, NText } from 'naive-ui'
import { sendChat } from '../api'
import type { ChatMessage } from '../types'
import type { ChatMessagePayload } from '../api'
import MessageBubble from './MessageBubble.vue'

const props = defineProps<{ agentId: string }>()

const messages = ref<ChatMessage[]>([])
const inputText = ref('')
const loading = ref(false)
const messagesContainer = ref<HTMLElement | null>(null)

// Reset conversation when agent changes
watch(
  () => props.agentId,
  () => {
    messages.value = []
    inputText.value = ''
  },
)

const canSend = computed(() => inputText.value.trim().length > 0 && !loading.value)

async function handleSend() {
  const text = inputText.value.trim()
  if (!text || loading.value) return

  // Add user message
  const userMsg: ChatMessage = {
    role: 'user',
    content: text,
    timestamp: Date.now(),
  }
  messages.value.push(userMsg)
  inputText.value = ''
  loading.value = true

  await scrollToBottom()

  // Build history for API
  const history: ChatMessagePayload[] = messages.value
    .filter((m) => m !== userMsg)
    .map((m) => ({ role: m.role, content: m.content }))

  try {
    const response = await sendChat(props.agentId, text, history)
    const assistantMsg: ChatMessage = {
      role: 'assistant',
      content: response.message,
      sources: response.sources,
      latency_ms: response.latency_ms,
      timestamp: Date.now(),
    }
    messages.value.push(assistantMsg)
  } catch (e) {
    const errorMsg: ChatMessage = {
      role: 'assistant',
      content: `请求失败: ${e instanceof Error ? e.message : '未知错误'}`,
      timestamp: Date.now(),
    }
    messages.value.push(errorMsg)
  } finally {
    loading.value = false
    await scrollToBottom()
  }
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
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
  }
}
</script>

<template>
  <div style="display: flex; flex-direction: column; height: 100vh">
    <!-- Messages area -->
    <div
      ref="messagesContainer"
      style="
        flex: 1;
        overflow-y: auto;
        padding: 20px;
        display: flex;
        flex-direction: column;
        gap: 16px;
      "
    >
      <div
        v-if="messages.length === 0"
        style="
          flex: 1;
          display: flex;
          align-items: center;
          justify-content: center;
          flex-direction: column;
          gap: 8px;
        "
      >
        <NText depth="3" style="font-size: 24px">💬</NText>
        <NText depth="3">开始与智能体对话</NText>
        <NText depth="3" style="font-size: 12px">输入消息开始聊天</NText>
      </div>

      <MessageBubble v-for="(msg, idx) in messages" :key="idx" :message="msg" />

      <!-- Thinking indicator -->
      <div v-if="loading" style="display: flex; align-items: center; gap: 8px; padding-left: 8px">
        <NSpin :size="16" />
        <NText depth="3" style="font-size: 13px">思考中...</NText>
      </div>
    </div>

    <!-- Input area -->
    <div
      style="
        padding: 16px 20px;
        border-top: 1px solid #30363d;
        background: #0d1117;
      "
    >
      <NSpace :size="8" align="end">
        <NInput
          v-model:value="inputText"
          type="textarea"
          placeholder="输入消息... (Enter 发送, Shift+Enter 换行)"
          :autosize="{ minRows: 1, maxRows: 5 }"
          style="flex: 1"
          @keydown="handleKeydown"
        />
        <NButton type="primary" :disabled="!canSend" @click="handleSend">
          发送
        </NButton>
      </NSpace>
    </div>
  </div>
</template>
