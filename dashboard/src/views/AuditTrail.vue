<script setup lang="ts">
import { ref, watch } from 'vue'
import { NCard, NSpace, NSelect, NDataTable, NTag, NEmpty, NSpin, NButton, type DataTableColumns } from 'naive-ui'
import { getAuditEvents } from '@/api/client'
import type { AuditEvent } from '@/types'

const events = ref<AuditEvent[]>([])
const total = ref(0)
const loading = ref(false)
const filterAgent = ref<string | null>(null)
const filterType = ref<string | null>(null)

const agentOptions = [
  { label: '全部智能体', value: '' },
  { label: 'coder', value: 'coder' },
  { label: 'researcher', value: 'researcher' },
  { label: 'writer', value: 'writer' },
  { label: '_shared', value: '_shared' },
]

const typeOptions = [
  { label: '全部类型', value: '' },
  { label: 'vector_insert', value: 'vector_insert' },
  { label: 'cache_put', value: 'cache_put' },
  { label: 'cache_evict', value: 'cache_evict' },
  { label: 'graph_add_node', value: 'graph_add_node' },
  { label: 'graph_add_edge', value: 'graph_add_edge' },
  { label: 'promote', value: 'promote' },
  { label: 'demote', value: 'demote' },
  { label: 'agent_switch', value: 'agent_switch' },
]

function eventColor(type: string): 'success' | 'info' | 'warning' | 'error' | 'default' {
  if (type.startsWith('vector_insert') || type.startsWith('graph_add')) return 'success'
  if (type.startsWith('cache_get') || type.startsWith('vector_search')) return 'info'
  if (type === 'promote' || type === 'demote') return 'warning'
  if (type.includes('delete') || type.includes('evict')) return 'error'
  return 'default'
}

const columns: DataTableColumns<AuditEvent> = [
  { title: 'ID', key: 'id', width: 60 },
  { title: '时间戳', key: 'timestamp', width: 180, render: (row) => new Date(row.timestamp).toLocaleString() },
  { title: '事件类型', key: 'event_type', width: 140 },
  { title: '智能体', key: 'agent_id', width: 100 },
  { title: '记忆ID', key: 'memory_id', width: 100, render: (row) => row.memory_id?.slice(0, 8) ?? '-' },
  { title: '层级', key: 'layer', width: 60, render: (row) => row.layer ?? '-' },
]

async function refresh() {
  loading.value = true
  try {
    const res = await getAuditEvents({
      agentId: filterAgent.value || undefined,
      eventType: filterType.value || undefined,
      limit: 100,
    })
    events.value = res.events
    total.value = res.total
  } finally {
    loading.value = false
  }
}

watch([filterAgent, filterType], refresh, { immediate: true })
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">审计日志</h2>
      <NSelect
        v-model:value="filterAgent"
        :options="agentOptions"
        style="width: 150px"
        size="small"
        placeholder="筛选智能体"
        clearable
      />
      <NSelect
        v-model:value="filterType"
        :options="typeOptions"
        style="width: 180px"
        size="small"
        placeholder="筛选类型"
        clearable
      />
      <NButton @click="refresh" size="small" ghost>刷新</NButton>
      <NTag type="info" size="small">共 {{ total }} 条事件</NTag>
    </NSpace>

    <NCard size="small">
      <NSpin :show="loading">
        <NDataTable
          v-if="events.length > 0"
          :columns="columns"
          :data="events"
          :row-key="(row: AuditEvent) => row.id"
          :max-height="500"
          striped
          size="small"
        />
        <NEmpty v-else description="暂无审计事件。请先填充演示数据以生成事件。" />
      </NSpin>
    </NCard>
  </NSpace>
</template>
