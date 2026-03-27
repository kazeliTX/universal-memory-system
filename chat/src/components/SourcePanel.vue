<script setup lang="ts">
import { computed } from 'vue'
import { NCard, NTag, NText } from 'naive-ui'
import type { ChatSource } from '../types'

const props = defineProps<{ sources: ChatSource[] }>()

const sortedSources = computed(() =>
  [...props.sources].sort((a, b) => b.score - a.score),
)

function scoreColor(score: number): string {
  if (score >= 0.8) return '#3fb950'
  if (score >= 0.5) return '#d29922'
  return '#8b949e'
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text
  return text.slice(0, max) + '...'
}
</script>

<template>
  <div style="margin-top: 10px; display: flex; flex-direction: column; gap: 6px">
    <NCard
      v-for="(src, idx) in sortedSources"
      :key="idx"
      size="small"
      :style="{ background: '#161b22', border: '1px solid #30363d' }"
    >
      <div style="display: flex; align-items: flex-start; gap: 8px">
        <NTag
          :bordered="false"
          size="small"
          :style="{ background: scoreColor(src.score) + '22', color: scoreColor(src.score) }"
        >
          {{ src.score.toFixed(2) }}
        </NTag>
        <div style="flex: 1; min-width: 0">
          <NText style="font-size: 13px; line-height: 1.5; display: block">
            {{ truncate(src.content, 200) }}
          </NText>
          <NText depth="3" style="font-size: 11px; margin-top: 4px; display: block">
            ID: {{ src.memory_id }}
          </NText>
        </div>
      </div>
    </NCard>
  </div>
</template>
