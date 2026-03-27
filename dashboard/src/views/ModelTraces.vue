<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, h } from 'vue'
import {
  NCard,
  NSpace,
  NSelect,
  NDataTable,
  NTag,
  NEmpty,
  NSpin,
  NButton,
  NInputNumber,
  NSwitch,
  NStatistic,
  NGrid,
  NGi,
  NText,
  type DataTableColumns,
} from 'naive-ui'
import { listTraces, traceSummary, clearTraces } from '@/api/client'
import type {
  ModelTraceResponse,
  TraceSummaryResponse,
  ModelTraceStatResponse,
  TaskTraceStatResponse,
} from '@/types'

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const traces = ref<ModelTraceResponse[]>([])
const summary = ref<TraceSummaryResponse | null>(null)
const loading = ref(false)
const filterModel = ref<string | null>(null)
const filterTask = ref<string | null>(null)
const limit = ref(50)
const autoRefresh = ref(false)
const expandedRowKeys = ref<string[]>([])

let refreshTimer: ReturnType<typeof setInterval> | null = null

// ---------------------------------------------------------------------------
// Filter options (derived from summary data)
// ---------------------------------------------------------------------------

const modelOptions = ref<{ label: string; value: string }[]>([])
const taskOptions = ref<{ label: string; value: string }[]>([])

function updateFilterOptions(s: TraceSummaryResponse) {
  modelOptions.value = [
    { label: '全部模型', value: '' },
    ...s.by_model.map((m) => ({ label: m.model_id, value: m.model_id })),
  ]
  taskOptions.value = [
    { label: '全部任务', value: '' },
    ...s.by_task.map((t) => ({ label: t.task, value: t.task })),
  ]
}

// ---------------------------------------------------------------------------
// Trace list table columns
// ---------------------------------------------------------------------------

const traceColumns: DataTableColumns<ModelTraceResponse> = [
  {
    title: '时间戳',
    key: 'timestamp',
    width: 180,
    render: (row) => new Date(row.timestamp).toLocaleString(),
  },
  { title: '模型', key: 'model_id', width: 130 },
  { title: '任务', key: 'task', width: 110 },
  { title: '请求类型', key: 'request_type', width: 100 },
  {
    title: '输入预览',
    key: 'input_preview',
    ellipsis: { tooltip: true },
    width: 200,
  },
  {
    title: '状态',
    key: 'success',
    width: 80,
    render: (row) =>
      h(
        NTag,
        { type: row.success ? 'success' : 'error', size: 'small' },
        { default: () => (row.success ? '成功' : '失败') },
      ),
  },
  {
    title: '延迟(ms)',
    key: 'latency_ms',
    width: 100,
    sorter: (a, b) => a.latency_ms - b.latency_ms,
  },
  { title: '重试', key: 'retry_count', width: 60 },
]

// Per-model table columns
const modelStatColumns: DataTableColumns<ModelTraceStatResponse> = [
  { title: '模型ID', key: 'model_id', width: 160 },
  { title: '请求数', key: 'count', width: 100 },
  { title: '错误数', key: 'errors', width: 100 },
  {
    title: '平均延迟(ms)',
    key: 'avg_latency_ms',
    width: 130,
    render: (row) => row.avg_latency_ms.toFixed(1),
  },
]

// Per-task table columns
const taskStatColumns: DataTableColumns<TaskTraceStatResponse> = [
  { title: '任务类型', key: 'task', width: 160 },
  { title: '请求数', key: 'count', width: 100 },
  { title: '错误数', key: 'errors', width: 100 },
  {
    title: '平均延迟(ms)',
    key: 'avg_latency_ms',
    width: 130,
    render: (row) => row.avg_latency_ms.toFixed(1),
  },
]

// ---------------------------------------------------------------------------
// Data fetching
// ---------------------------------------------------------------------------

async function refresh() {
  loading.value = true
  try {
    const [traceRes, summaryRes] = await Promise.all([
      listTraces(limit.value, filterModel.value || undefined, filterTask.value || undefined),
      traceSummary(),
    ])
    traces.value = traceRes.traces
    summary.value = summaryRes
    updateFilterOptions(summaryRes)
  } finally {
    loading.value = false
  }
}

async function handleClear() {
  await clearTraces()
  await refresh()
}

// ---------------------------------------------------------------------------
// Row expand for detail
// ---------------------------------------------------------------------------

function renderExpand(row: ModelTraceResponse) {
  const items: { label: string; value: string }[] = [
    { label: '完整输入', value: row.input_preview },
    { label: '输入Token估计', value: String(row.input_tokens_estimate) },
    { label: '调用方', value: row.caller },
  ]
  if (row.output_preview) {
    items.push({ label: '输出预览', value: row.output_preview })
  }
  if (row.output_dimension != null) {
    items.push({ label: '向量维度', value: String(row.output_dimension) })
  }
  if (row.output_tokens_estimate != null) {
    items.push({ label: '输出Token估计', value: String(row.output_tokens_estimate) })
  }
  if (row.error_message) {
    items.push({ label: '错误信息', value: row.error_message })
  }

  return h(
    'div',
    { style: 'padding: 8px 16px; font-size: 13px; line-height: 1.8;' },
    items.map((item) =>
      h('div', { key: item.label }, [
        h('span', { style: 'color: #8b949e; margin-right: 8px;' }, `${item.label}:`),
        h(
          'span',
          {
            style:
              item.label === '错误信息'
                ? 'color: #f85149; word-break: break-all;'
                : 'color: #e6edf3; word-break: break-all;',
          },
          item.value,
        ),
      ]),
    ),
  )
}

// ---------------------------------------------------------------------------
// Row class — highlight failed rows
// ---------------------------------------------------------------------------

function rowClassName(row: ModelTraceResponse) {
  return row.success ? '' : 'trace-row-error'
}

// ---------------------------------------------------------------------------
// Auto-refresh
// ---------------------------------------------------------------------------

watch(autoRefresh, (val) => {
  if (val) {
    refreshTimer = setInterval(refresh, 5000)
  } else if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
})

watch([filterModel, filterTask, limit], refresh)

onMounted(refresh)

onUnmounted(() => {
  if (refreshTimer) {
    clearInterval(refreshTimer)
  }
})
</script>

<template>
  <NSpace vertical :size="16">
    <!-- Header -->
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">模型追踪</h2>
      <NButton size="small" @click="refresh" :loading="loading">刷新</NButton>
      <NButton size="small" type="error" @click="handleClear">清空</NButton>
      <NSpace align="center" :size="8">
        <NText style="color: #8b949e; font-size: 13px">自动刷新</NText>
        <NSwitch v-model:value="autoRefresh" size="small" />
      </NSpace>
    </NSpace>

    <!-- Summary cards -->
    <NGrid :cols="4" :x-gap="12" v-if="summary">
      <NGi>
        <NCard size="small">
          <NStatistic label="总请求数" :value="summary.total_traces" />
        </NCard>
      </NGi>
      <NGi>
        <NCard size="small">
          <NStatistic label="错误数" :value="summary.total_errors">
            <template #suffix>
              <NText v-if="summary.total_traces > 0" style="font-size: 13px; color: #8b949e">
                ({{ ((summary.total_errors / summary.total_traces) * 100).toFixed(1) }}%)
              </NText>
            </template>
          </NStatistic>
        </NCard>
      </NGi>
      <NGi>
        <NCard size="small">
          <NStatistic label="平均延迟" :value="summary.avg_latency_ms.toFixed(1)">
            <template #suffix>ms</template>
          </NStatistic>
        </NCard>
      </NGi>
      <NGi>
        <NCard size="small">
          <NStatistic label="P99延迟" :value="summary.p99_latency_ms.toFixed(1)">
            <template #suffix>ms</template>
          </NStatistic>
        </NCard>
      </NGi>
    </NGrid>

    <!-- Breakdown tables -->
    <NGrid :cols="2" :x-gap="12" v-if="summary">
      <NGi>
        <NCard title="按模型统计" size="small">
          <NDataTable
            :columns="modelStatColumns"
            :data="summary.by_model"
            :bordered="false"
            size="small"
            :max-height="200"
          />
        </NCard>
      </NGi>
      <NGi>
        <NCard title="按任务统计" size="small">
          <NDataTable
            :columns="taskStatColumns"
            :data="summary.by_task"
            :bordered="false"
            size="small"
            :max-height="200"
          />
        </NCard>
      </NGi>
    </NGrid>

    <!-- Filters -->
    <NSpace align="center" :size="12">
      <NSelect
        v-model:value="filterModel"
        :options="modelOptions"
        style="width: 180px"
        size="small"
        placeholder="筛选模型"
        clearable
      />
      <NSelect
        v-model:value="filterTask"
        :options="taskOptions"
        style="width: 180px"
        size="small"
        placeholder="筛选任务"
        clearable
      />
      <NSpace align="center" :size="4">
        <NText style="color: #8b949e; font-size: 13px">限制:</NText>
        <NInputNumber v-model:value="limit" :min="10" :max="500" :step="10" size="small" style="width: 100px" />
      </NSpace>
    </NSpace>

    <!-- Trace list -->
    <NSpin :show="loading">
      <NCard title="请求追踪列表" size="small">
        <NDataTable
          v-if="traces.length > 0"
          :columns="traceColumns"
          :data="traces"
          :bordered="false"
          size="small"
          :max-height="500"
          :row-key="(row: ModelTraceResponse) => row.id"
          :row-class-name="rowClassName"
          :expanded-row-keys="expandedRowKeys"
          @update:expanded-row-keys="(keys: Array<string | number>) => (expandedRowKeys = keys.map(String))"
          :render-expand="renderExpand"
        />
        <NEmpty v-else description="暂无追踪记录" />
      </NCard>
    </NSpin>
  </NSpace>
</template>

<style>
.trace-row-error td {
  background-color: rgba(248, 81, 73, 0.1) !important;
}
</style>
