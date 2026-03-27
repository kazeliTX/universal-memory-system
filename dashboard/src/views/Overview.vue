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
      <h2 style="margin: 0; color: #e6edf3">系统概览</h2>
      <NSpace>
        <NButton @click="handleSeed" :loading="seeding" type="primary" ghost size="small">
          填充演示数据
        </NButton>
        <NButton @click="handleClear" :loading="clearing" type="error" ghost size="small">
          清空所有数据
        </NButton>
        <NTag v-if="seedResult" type="success" size="small">
          已填充: {{ seedResult.memories }} 条记忆, {{ seedResult.nodes }} 个节点
        </NTag>
      </NSpace>
    </NSpace>

    <NCard v-if="error" title="连接错误" size="small">
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
        <NCard title="缓存" size="small">
          <NStatistic label="L0 (感知层)" :value="stats.cache.l0_entries" />
          <NStatistic label="L1 (工作层)" :value="stats.cache.l1_entries" />
        </NCard>
      </NGi>
      <NGi>
        <NCard title="向量存储 (L2)" size="small">
          <NStatistic label="总条目数" :value="stats.vector.total_entries" />
        </NCard>
      </NGi>
      <NGi>
        <NCard title="图谱存储 (L3)" size="small">
          <NStatistic label="节点数" :value="stats.graph.total_nodes" />
          <NStatistic label="边数" :value="stats.graph.total_edges" />
          <NStatistic label="共享节点" :value="stats.graph.shared_nodes" />
        </NCard>
      </NGi>
      <NGi>
        <NCard title="系统" size="small">
          <NStatistic label="运行时间" :value="health ? formatUptime(health.uptime_secs) : '-'" />
          <NStatistic label="智能体" :value="stats.agents.length" />
        </NCard>
      </NGi>
    </NGrid>

    <!-- Encoder -->
    <NCard title="编码器 (M2)" size="small" v-if="encoder">
      <NGrid :cols="5" :x-gap="16">
        <NGi>
          <NSpace align="center" :size="8">
            <NBadge :type="encoder.available ? 'success' : 'error'" dot />
            <NStatistic label="状态" :value="encoder.available ? '在线' : '离线'" />
          </NSpace>
        </NGi>
        <NGi>
          <NStatistic label="模型" :value="encoder.model ?? '-'" />
        </NGi>
        <NGi>
          <NStatistic label="维度" :value="encoder.dimension ?? '-'" />
        </NGi>
        <NGi>
          <NStatistic label="已编码文本" :value="encoder.total_texts_encoded" />
        </NGi>
        <NGi>
          <NStatistic label="平均延迟" :value="encoder.avg_latency_ms > 0 ? encoder.avg_latency_ms.toFixed(0) + ' ms' : '-'" />
        </NGi>
      </NGrid>
      <NSpace style="margin-top: 12px" :size="12" v-if="encoder.available">
        <NTag type="info" size="small">请求数: {{ encoder.total_requests }}</NTag>
        <NTag :type="encoder.total_errors > 0 ? 'error' : 'success'" size="small">
          错误数: {{ encoder.total_errors }}
        </NTag>
        <NTag :type="encoder.total_retries > 0 ? 'warning' : 'success'" size="small">
          重试数: {{ encoder.total_retries }}
        </NTag>
      </NSpace>
    </NCard>

    <!-- Agent Cards -->
    <NCard title="智能体" size="small" v-if="stats">
      <NSpace>
        <NTag v-for="agent in stats.agents" :key="agent" type="info" size="medium" round>
          {{ agent }}
        </NTag>
      </NSpace>
    </NCard>
  </NSpace>
</template>
