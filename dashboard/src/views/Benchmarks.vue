<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { NCard, NSpace, NDataTable, NEmpty, NSpin, NButton, NTag, type DataTableColumns } from 'naive-ui'
import { getBenchmarks } from '@/api/client'
import type { BenchmarkEntry } from '@/types'

const benchmarks = ref<BenchmarkEntry[]>([])
const loading = ref(false)

function formatTime(ns: number): string {
  if (ns < 1_000) return `${ns.toFixed(0)} ns`
  if (ns < 1_000_000) return `${(ns / 1_000).toFixed(1)} µs`
  if (ns < 1_000_000_000) return `${(ns / 1_000_000).toFixed(1)} ms`
  return `${(ns / 1_000_000_000).toFixed(2)} s`
}

const columns: DataTableColumns<BenchmarkEntry> = [
  { title: 'Benchmark', key: 'name' },
  { title: 'Mean', key: 'mean_ns', width: 120, render: (row) => formatTime(row.mean_ns) },
  { title: 'Median', key: 'median_ns', width: 120, render: (row) => formatTime(row.median_ns) },
  { title: 'Std Dev', key: 'std_dev_ns', width: 120, render: (row) => formatTime(row.std_dev_ns) },
]

const maxMean = computed(() => {
  if (benchmarks.value.length === 0) return 1
  return Math.max(...benchmarks.value.map((b) => b.mean_ns))
})

async function refresh() {
  loading.value = true
  try {
    const res = await getBenchmarks()
    benchmarks.value = res.benchmarks
  } finally {
    loading.value = false
  }
}

onMounted(refresh)
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">Performance Benchmarks</h2>
      <NButton @click="refresh" :loading="loading" size="small" ghost>Refresh</NButton>
      <NTag type="info" size="small">{{ benchmarks.length }} benchmarks</NTag>
    </NSpace>

    <NCard size="small">
      <NSpin :show="loading">
        <NDataTable
          v-if="benchmarks.length > 0"
          :columns="columns"
          :data="benchmarks"
          :row-key="(row: BenchmarkEntry) => row.name"
          striped
          size="small"
        />
        <NEmpty v-else description="No benchmark data. Run: cargo bench -p umms-storage" />
      </NSpin>
    </NCard>

    <!-- Visual bars -->
    <NCard title="Relative Performance" size="small" v-if="benchmarks.length > 0">
      <div v-for="b in benchmarks" :key="b.name" style="margin-bottom: 8px">
        <div style="display: flex; align-items: center; gap: 12px">
          <span style="width: 260px; font-size: 12px; color: #8b949e; text-align: right">
            {{ b.name }}
          </span>
          <div style="flex: 1; height: 20px; background: #21262d; border-radius: 4px; overflow: hidden">
            <div
              :style="{
                width: ((b.mean_ns / maxMean) * 100).toFixed(1) + '%',
                height: '100%',
                background: 'linear-gradient(90deg, #58a6ff, #3fb950)',
                borderRadius: '4px',
                transition: 'width 0.3s',
              }"
            />
          </div>
          <span style="width: 80px; font-size: 12px; color: #e6edf3">
            {{ formatTime(b.mean_ns) }}
          </span>
        </div>
      </div>
    </NCard>
  </NSpace>
</template>
