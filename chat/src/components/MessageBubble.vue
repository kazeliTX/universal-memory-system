<script setup lang="ts">
import { ref } from 'vue'
import { NText } from 'naive-ui'
import type { ChatMessage } from '../types'
import SourcePanel from './SourcePanel.vue'

const props = defineProps<{ message: ChatMessage }>()

const showSources = ref(false)

function formatTime(ts: number): string {
  const d = new Date(ts)
  return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}
</script>

<template>
  <div
    :style="{
      display: 'flex',
      justifyContent: props.message.role === 'user' ? 'flex-end' : 'flex-start',
    }"
  >
    <div
      :style="{
        maxWidth: '75%',
        padding: '12px 16px',
        borderRadius: '12px',
        background: props.message.role === 'user' ? '#1f6feb' : '#21262d',
        color: '#c9d1d9',
      }"
    >
      <!-- Message content -->
      <div style="white-space: pre-wrap; word-break: break-word; line-height: 1.6">
        {{ props.message.content }}
      </div>

      <!-- Footer: time + latency + sources toggle -->
      <div
        style="
          display: flex;
          align-items: center;
          gap: 8px;
          margin-top: 8px;
          flex-wrap: wrap;
        "
      >
        <NText depth="3" style="font-size: 11px">
          {{ formatTime(props.message.timestamp) }}
        </NText>

        <NText
          v-if="props.message.latency_ms"
          depth="3"
          style="font-size: 11px"
        >
          {{ props.message.latency_ms }}ms
        </NText>

        <NText
          v-if="props.message.sources && props.message.sources.length > 0"
          style="font-size: 12px; cursor: pointer; color: #58a6ff"
          @click="showSources = !showSources"
        >
          📚 {{ showSources ? '隐藏来源' : '查看来源' }} ({{ props.message.sources.length }})
        </NText>
      </div>

      <!-- Source panel -->
      <SourcePanel
        v-if="showSources && props.message.sources && props.message.sources.length > 0"
        :sources="props.message.sources"
      />
    </div>
  </div>
</template>
