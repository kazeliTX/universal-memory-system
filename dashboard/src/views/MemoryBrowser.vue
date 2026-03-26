<script setup lang="ts">
import { ref, watch, computed, h } from 'vue'
import {
  NCard, NSpace, NSelect, NRadioGroup, NRadioButton,
  NDataTable, NTag, NEmpty, NSpin, NInput, NButton,
  NProgress, NTooltip, NStatistic, NGrid, NGi,
  type DataTableColumns,
} from 'naive-ui'
import { getCacheEntries, getVectorEntries, semanticSearch, epaAnalyze } from '@/api/client'
import type { MemoryEntry, SemanticSearchResponse, SearchHit, EpaAnalyzeResponse } from '@/types'

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
const epaResult = ref<EpaAnalyzeResponse | null>(null)

const columns: DataTableColumns<MemoryEntry> = [
  { title: 'ID', key: 'id', width: 100, ellipsis: { tooltip: true }, render: (row) => row.id.slice(0, 8) + '\u2026' },
  { title: 'Content', key: 'content_text', ellipsis: { tooltip: true } },
  { title: 'Importance', key: 'importance', width: 100, render: (row) => row.importance.toFixed(2) },
  { title: 'Scope', key: 'scope', width: 90 },
  { title: 'Modality', key: 'modality', width: 90 },
  { title: 'Tags', key: 'tags', width: 150, render: (row) => row.tags.join(', ') },
  { title: 'Created', key: 'created_at', width: 180, render: (row) => new Date(row.created_at).toLocaleString() },
]

// Source tag colors
function sourceTag(source: string) {
  const map: Record<string, { type: any; label: string }> = {
    both: { type: 'success', label: 'BM25+Vec' },
    bm25_only: { type: 'warning', label: 'BM25' },
    vector_only: { type: 'info', label: 'Vector' },
    diffusion: { type: 'default', label: 'Diffusion' },
    unknown: { type: 'default', label: '?' },
  }
  return map[source] ?? map.unknown!
}

const searchColumns: DataTableColumns<SearchHit> = [
  {
    title: 'Score',
    key: 'score',
    width: 80,
    render: (row) => row.score.toFixed(4),
  },
  {
    title: 'Source',
    key: 'source',
    width: 110,
    render: (row) => {
      const tag = sourceTag(row.source)
      const parts: string[] = []
      if (row.bm25_rank) parts.push(`BM25 #${row.bm25_rank}`)
      if (row.vector_rank) parts.push(`Vec #${row.vector_rank}`)
      return h(NTooltip, null, {
        trigger: () => h(NTag, { type: tag.type, size: 'small', round: true }, () => tag.label),
        default: () => parts.join(' | ') || 'Unknown source',
      })
    },
  },
  {
    title: 'Contribution',
    key: 'contribution',
    width: 160,
    render: (row) => {
      const total = row.bm25_contribution + row.vector_contribution
      const bm25Pct = total > 0 ? (row.bm25_contribution / total) * 100 : 0
      return h('div', { style: 'display:flex;align-items:center;gap:4px;font-size:11px' }, [
        h('span', { style: 'color:#f0a020;width:28px' }, `${bm25Pct.toFixed(0)}%`),
        h('div', { style: 'flex:1;height:8px;background:#333;border-radius:4px;overflow:hidden;display:flex' }, [
          h('div', { style: `width:${bm25Pct}%;background:#f0a020;height:100%` }),
          h('div', { style: `width:${100 - bm25Pct}%;background:#18a058;height:100%` }),
        ]),
        h('span', { style: 'color:#18a058;width:28px;text-align:right' }, `${(100 - bm25Pct).toFixed(0)}%`),
      ])
    },
  },
  {
    title: 'Agent',
    key: 'agent',
    width: 90,
    render: (row) => row.entry.agent_id,
  },
  {
    title: 'Content',
    key: 'content',
    ellipsis: { tooltip: true },
    render: (row) => row.entry.content_text ?? '-',
  },
  {
    title: 'Scope',
    key: 'scope',
    width: 80,
    render: (row) => row.entry.scope,
  },
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
  epaResult.value = null
  try {
    searchResult.value = await semanticSearch(
      searchQuery.value,
      selectedAgent.value,
      10,
      true,
    )
    // Fire EPA analysis in parallel (non-blocking)
    epaAnalyze(searchQuery.value, selectedAgent.value)
      .then((res) => { epaResult.value = res })
      .catch(() => { /* EPA unavailable, ignore */ })
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
    <NCard title="Hybrid Search" size="small">
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
      </NSpace>

      <!-- Pipeline Stats -->
      <div v-if="searchResult" style="margin-top: 16px">
        <!-- Stage Timeline -->
        <div style="display: flex; gap: 8px; margin-bottom: 12px; align-items: stretch">
          <NCard size="small" style="flex: 1; text-align: center">
            <div style="color: #999; font-size: 11px">Encode</div>
            <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ searchResult.latency.encode_ms }}ms</div>
          </NCard>
          <div style="display: flex; align-items: center; color: #555">&rarr;</div>
          <NCard size="small" style="flex: 1; text-align: center">
            <div style="color: #999; font-size: 11px">Recall</div>
            <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ searchResult.latency.recall_ms }}ms</div>
            <div style="color: #666; font-size: 11px">{{ searchResult.pipeline.recall_count }} hits</div>
          </NCard>
          <div style="display: flex; align-items: center; color: #555">&rarr;</div>
          <NCard size="small" style="flex: 1; text-align: center">
            <div style="color: #999; font-size: 11px">Rerank</div>
            <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ searchResult.latency.rerank_ms }}ms</div>
            <div style="color: #666; font-size: 11px">{{ searchResult.pipeline.rerank_count }} kept</div>
          </NCard>
          <div style="display: flex; align-items: center; color: #555">&rarr;</div>
          <NCard size="small" style="flex: 1; text-align: center">
            <div style="color: #999; font-size: 11px">Diffusion</div>
            <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ searchResult.latency.diffusion_ms }}ms</div>
            <div style="color: #666; font-size: 11px">+{{ searchResult.pipeline.diffusion_count }} found</div>
          </NCard>
          <div style="display: flex; align-items: center; color: #555">&rarr;</div>
          <NCard size="small" style="flex: 1; text-align: center; border-color: #18a058">
            <div style="color: #18a058; font-size: 11px">Total</div>
            <div style="color: #18a058; font-size: 16px; font-weight: bold">{{ searchResult.latency.total_ms }}ms</div>
            <div style="color: #666; font-size: 11px">{{ searchResult.pipeline.final_count }} results</div>
          </NCard>
        </div>

        <!-- Source Distribution -->
        <div style="display: flex; gap: 16px; margin-bottom: 12px">
          <NTag type="success" size="small" round>
            BM25+Vec: {{ searchResult.pipeline.both }}
          </NTag>
          <NTag type="warning" size="small" round>
            BM25 only: {{ searchResult.pipeline.bm25_only }}
          </NTag>
          <NTag type="info" size="small" round>
            Vec only: {{ searchResult.pipeline.vector_only }}
          </NTag>
          <NTag v-if="searchResult.pipeline.diffusion_count > 0" size="small" round>
            Diffusion: {{ searchResult.pipeline.diffusion_count }}
          </NTag>
        </div>

        <!-- EPA Metrics -->
        <div v-if="epaResult" style="margin-bottom: 12px">
          <div style="display: flex; gap: 8px; align-items: stretch">
            <NCard size="small" style="flex: 1; text-align: center">
              <div style="color: #999; font-size: 11px">Logic Depth</div>
              <div style="color: #58a6ff; font-size: 16px; font-weight: bold">{{ epaResult.logic_depth.toFixed(3) }}</div>
            </NCard>
            <NCard size="small" style="flex: 1; text-align: center">
              <div style="color: #999; font-size: 11px">Cross-Domain</div>
              <div style="color: #d2a8ff; font-size: 16px; font-weight: bold">{{ epaResult.cross_domain_resonance.toFixed(3) }}</div>
            </NCard>
            <NCard size="small" style="flex: 1; text-align: center">
              <div style="color: #999; font-size: 11px">Alpha</div>
              <div style="color: #f0a020; font-size: 16px; font-weight: bold">{{ epaResult.alpha.toFixed(3) }}</div>
            </NCard>
            <NCard size="small" style="flex: 1; text-align: center">
              <div style="color: #999; font-size: 11px">Axes</div>
              <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ epaResult.num_semantic_axes }}</div>
            </NCard>
          </div>
          <div v-if="epaResult.activated_tags.length > 0" style="margin-top: 8px; display: flex; gap: 6px; flex-wrap: wrap">
            <NTag
              v-for="at in epaResult.activated_tags.slice(0, 15)"
              :key="at.tag_id"
              size="small"
              round
              type="success"
            >
              {{ at.label }} ({{ at.similarity.toFixed(2) }})
            </NTag>
          </div>
        </div>

        <!-- Results Table -->
        <NDataTable
          v-if="searchResult.results.length > 0"
          :columns="searchColumns"
          :data="searchResult.results"
          :row-key="(row: SearchHit) => row.entry.id"
          :max-height="400"
          striped
          size="small"
        />
        <NEmpty v-else description="No matching memories found." />
      </div>
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
