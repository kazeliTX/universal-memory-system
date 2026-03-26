<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { NGrid, NGi, NCard, NStatistic, NTag, NButton, NSpace, NBadge } from 'naive-ui'
import { getHealth, getStats, getEncoderStatus, seedDemo, clearDemo } from '@/api/client'
import type { HealthResponse, StatsResponse, SeedResponse, EncoderStatusResponse } from '@/types'

const health = ref<HealthResponse | null>(null)
const stats = ref<StatsResponse | null>(null)
const encoder = ref<EncoderStatusResponse | null>(null)
const seeding = ref(false)
const clearing = ref(false)
const seedResult = ref<SeedResponse | null>(null)
const error = ref<string | null>(null)

let timer: ReturnType<typeof setInterval>

async function refresh() {
  try {
    const [h, s, e] = await Promise.all([getHealth(), getStats(), getEncoderStatus()])
    health.value = h
    stats.value = s
    encoder.value = e
    error.value = null
  } catch (e: any) {
    error.value = e.message ?? 'Connection failed'
  }
}

async function handleSeed() {
  seeding.value = true
  try {
    seedResult.value = await seedDemo()
    await refresh()
  } finally {
    seeding.value = false
  }
}

async function handleClear() {
  clearing.value = true
  try {
    await clearDemo()
    seedResult.value = null
    await refresh()
  } finally {
    clearing.value = false
  }
}

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = secs % 60
  if (h > 0) return `${h}h ${m}m ${s}s`
  if (m > 0) return `${m}m ${s}s`
  return `${s}s`
}

onMounted(() => {
  refresh()
  timer = setInterval(refresh, 5000)
})

onUnmounted(() => clearInterval(timer))
</script>

<template>
  <NSpace vertical :size="24">
    <NSpace justify="space-between" align="center">
      <h2 style="margin: 0; color: #e6edf3">System Overview</h2>
      <NSpace>
        <NButton @click="handleSeed" :loading="seeding" type="primary" ghost size="small">
          Seed Demo Data
        </NButton>
        <NButton @click="handleClear" :loading="clearing" type="error" ghost size="small">
          Clear All Data
        </NButton>
        <NTag v-if="seedResult" type="success" size="small">
          Seeded: {{ seedResult.memories }} memories, {{ seedResult.nodes }} nodes
        </NTag>
      </NSpace>
    </NSpace>

    <NCard v-if="error" title="Connection Error" size="small">
      <NTag type="error">{{ error }}</NTag>
    </NCard>

    <!-- Health -->
    <NGrid :cols="4" :x-gap="16" :y-gap="16" v-if="health">
      <NGi v-for="(status, key) in health.storage" :key="key">
        <NCard size="small">
          <NSpace align="center" :size="8">
            <NBadge :type="status === 'ok' ? 'success' : 'error'" dot />
            <NStatistic :label="String(key)" :value="status" />
          </NSpace>
        </NCard>
      </NGi>
    </NGrid>

    <!-- Stats -->
    <NGrid :cols="4" :x-gap="16" :y-gap="16" v-if="stats">
      <NGi>
        <NCard title="Cache" size="small">
          <NStatistic label="L0 (Sensory)" :value="stats.cache.l0_entries" />
          <NStatistic label="L1 (Working)" :value="stats.cache.l1_entries" />
        </NCard>
      </NGi>
      <NGi>
        <NCard title="Vector Store (L2)" size="small">
          <NStatistic label="Total Entries" :value="stats.vector.total_entries" />
        </NCard>
      </NGi>
      <NGi>
        <NCard title="Knowledge Graph (L3)" size="small">
          <NStatistic label="Nodes" :value="stats.graph.total_nodes" />
          <NStatistic label="Edges" :value="stats.graph.total_edges" />
          <NStatistic label="Shared Nodes" :value="stats.graph.shared_nodes" />
        </NCard>
      </NGi>
      <NGi>
        <NCard title="System" size="small">
          <NStatistic label="Uptime" :value="health ? formatUptime(health.uptime_secs) : '-'" />
          <NStatistic label="Agents" :value="stats.agents.length" />
        </NCard>
      </NGi>
    </NGrid>

    <!-- Encoder -->
    <NCard title="Encoder (M2)" size="small" v-if="encoder">
      <NGrid :cols="5" :x-gap="16">
        <NGi>
          <NSpace align="center" :size="8">
            <NBadge :type="encoder.available ? 'success' : 'error'" dot />
            <NStatistic label="Status" :value="encoder.available ? 'Online' : 'Offline'" />
          </NSpace>
        </NGi>
        <NGi>
          <NStatistic label="Model" :value="encoder.model ?? '-'" />
        </NGi>
        <NGi>
          <NStatistic label="Dimension" :value="encoder.dimension ?? '-'" />
        </NGi>
        <NGi>
          <NStatistic label="Texts Encoded" :value="encoder.total_texts_encoded" />
        </NGi>
        <NGi>
          <NStatistic label="Avg Latency" :value="encoder.avg_latency_ms > 0 ? encoder.avg_latency_ms.toFixed(0) + ' ms' : '-'" />
        </NGi>
      </NGrid>
      <NSpace style="margin-top: 12px" :size="12" v-if="encoder.available">
        <NTag type="info" size="small">Requests: {{ encoder.total_requests }}</NTag>
        <NTag :type="encoder.total_errors > 0 ? 'error' : 'success'" size="small">
          Errors: {{ encoder.total_errors }}
        </NTag>
        <NTag :type="encoder.total_retries > 0 ? 'warning' : 'success'" size="small">
          Retries: {{ encoder.total_retries }}
        </NTag>
      </NSpace>
    </NCard>

    <!-- Agent Cards -->
    <NCard title="Agents" size="small" v-if="stats">
      <NSpace>
        <NTag v-for="agent in stats.agents" :key="agent" type="info" size="medium" round>
          {{ agent }}
        </NTag>
      </NSpace>
    </NCard>
  </NSpace>
</template>
