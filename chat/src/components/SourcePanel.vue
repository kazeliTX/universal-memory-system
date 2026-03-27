<script setup lang="ts">
import { computed } from 'vue'
import type { ChatSource } from '../types'

const props = defineProps<{ sources: ChatSource[] }>()

const sortedSources = computed(() =>
  [...props.sources].sort((a, b) => b.score - a.score),
)

function scoreColor(score: number): string {
  if (score >= 0.8) return '#00ff88'
  if (score >= 0.5) return '#00d4ff'
  return '#8b949e'
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text
  return text.slice(0, max) + '...'
}

function scorePercent(score: number): string {
  return (score * 100).toFixed(0) + '%'
}
</script>

<template>
  <div class="source-panel">
    <div v-for="(src, idx) in sortedSources" :key="idx" class="source-item">
      <div class="source-header">
        <span
          class="score-badge"
          :style="{
            background: scoreColor(src.score) + '1a',
            color: scoreColor(src.score),
            borderColor: scoreColor(src.score) + '44',
          }"
        >
          {{ scorePercent(src.score) }}
        </span>
        <div
          class="score-bar-track"
        >
          <div
            class="score-bar-fill"
            :style="{
              width: (src.score * 100) + '%',
              background: `linear-gradient(90deg, ${scoreColor(src.score)}88, ${scoreColor(src.score)})`,
            }"
          ></div>
        </div>
      </div>
      <div class="source-content">
        {{ truncate(src.content, 200) }}
      </div>
      <div class="source-id">
        ID: {{ src.memory_id }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.source-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 10px;
  animation: fadeIn 0.3s ease;
}

.source-item {
  padding: 10px 12px;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 8px;
}

.source-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}

.score-badge {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 4px;
  border: 1px solid;
  flex-shrink: 0;
}

.score-bar-track {
  flex: 1;
  height: 4px;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 2px;
  overflow: hidden;
}

.score-bar-fill {
  height: 100%;
  border-radius: 2px;
  transition: width 0.5s ease;
}

.source-content {
  font-size: 12px;
  line-height: 1.6;
  color: #8b949e;
  word-break: break-word;
}

.source-id {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  color: #484f58;
  margin-top: 6px;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>
