<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  NCard, NSpace, NSelect, NRadioGroup, NRadioButton,
  NDataTable, NTag, NEmpty, NSpin, NInput, NButton,
  NCollapse, NCollapseItem,
  type DataTableColumns,
} from 'naive-ui'
import { getCacheEntries, getVectorEntries, semanticSearch } from '@/api/client'
import type { MemoryEntry, SemanticSearchResponse } from '@/types'

const agents = ['coder', 'researcher', 'writer']
const selectedAgent = ref('coder')
const selectedLayer = ref<'cache' | 'vector'>('vector')
const loading = ref(false)
const entries = ref<MemoryEntry[]>([])
const totalCount = ref(0)

// Search state
const searchQuery = ref('')
const searching = ref(false)
const searchResult = ref<SemanticSearchResponse | null>(null)

const columns: DataTableColumns<MemoryEntry> = [
  { title: 'ID', key: 'id', width: 100, ellipsis: { tooltip: true }, render: (row) => row.id.slice(0, 8) + '\u2026' },
  { title: 'Content', key: 'content_text', ellipsis: { tooltip: true } },
  { title: 'Importance', key: 'importance', width: 100, render: (row) => row.importance.toFixed(2) },
  { title: 'Scope', key: 'scope', width: 90 },
  { title: 'Modality', key: 'modality', width: 90 },
  { title: 'Tags', key: 'tags', width: 150, render: (row) => row.tags.join(', ') },
  { title: 'Created', key: 'created_at', width: 180, render: (row) => new Date(row.created_at).toLocaleString() },
]

const searchColumns: DataTableColumns<{ entry: MemoryEntry; score: number }> = [
  { title: 'Score', key: 'score', width: 90, render: (row) => row.score.toFixed(4) },
  { title: 'Agent', key: 'agent', width: 100, render: (row) => row.entry.agent_id },
  { title: 'Content', key: 'content', ellipsis: { tooltip: true }, render: (row) => row.entry.content_text ?? '-' },
  { title: 'Scope', key: 'scope', width: 90, render: (row) => row.entry.scope },
  { title: 'Importance', key: 'importance', width: 100, render: (row) => row.entry.importance.toFixed(2) },
  { title: 'Tags', key: 'tags', width: 150, render: (row) => row.entry.tags.join(', ') },
]

async function refresh() {
  loading.value = true
  try {
    if (selectedLayer.value === 'cache') {
      const res = await getCacheEntries(selectedAgent.value)
      entries.value = [...res.l0, ...res.l1]
      totalCount.value = entries.value.length
    } else {
      const res = await getVectorEntries(selectedAgent.value, 0, 50)
      entries.value = res.entries
      totalCount.value = Number(res.total)
    }
  } finally {
    loading.value = false
  }
}

async function handleSearch() {
  if (!searchQuery.value.trim()) return
  searching.value = true
  try {
    searchResult.value = await semanticSearch(
      searchQuery.value,
      selectedAgent.value,
      5,
      true,
    )
  } finally {
    searching.value = false
  }
}

watch([selectedAgent, selectedLayer], refresh, { immediate: true })
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">Memory Browser</h2>
      <NSelect
        v-model:value="selectedAgent"
        :options="agents.map(a => ({ label: a, value: a }))"
        style="width: 160px"
        size="small"
      />
      <NRadioGroup v-model:value="selectedLayer" size="small">
        <NRadioButton value="cache">Cache (L0/L1)</NRadioButton>
        <NRadioButton value="vector">Vector (L2)</NRadioButton>
      </NRadioGroup>
      <NTag type="info" size="small">{{ totalCount }} entries</NTag>
    </NSpace>

    <!-- Semantic Search -->
    <NCard title="Semantic Search" size="small">
      <NSpace :size="12">
        <NInput
          v-model:value="searchQuery"
          placeholder="Enter a natural language query..."
          style="width: 500px"
          size="small"
          @keydown.enter="handleSearch"
        />
        <NButton
          type="primary"
          size="small"
          :loading="searching"
          @click="handleSearch"
          :disabled="!searchQuery.trim()"
        >
          Search
        </NButton>
        <NTag v-if="searchResult" type="success" size="small">
          {{ searchResult.results.length }} results in {{ searchResult.latency_ms }}ms
        </NTag>
      </NSpace>

      <div v-if="searchResult && searchResult.results.length > 0" style="margin-top: 12px">
        <NDataTable
          :columns="searchColumns"
          :data="searchResult.results"
          :row-key="(row: any) => row.entry.id"
          :max-height="300"
          striped
          size="small"
        />
      </div>
      <NEmpty v-else-if="searchResult && searchResult.results.length === 0"
        description="No matching memories found."
        style="margin-top: 12px"
      />
    </NCard>

    <!-- Memory Table -->
    <NCard size="small">
      <NSpin :show="loading">
        <NDataTable
          v-if="entries.length > 0"
          :columns="columns"
          :data="entries"
          :row-key="(row: MemoryEntry) => row.id"
          :max-height="500"
          striped
          size="small"
        />
        <NEmpty v-else description="No entries. Try seeding demo data." />
      </NSpin>
    </NCard>
  </NSpace>
</template>
