<script setup lang="ts">
import { ref, watch, h, computed } from 'vue'
import {
  NCard, NSpace, NSelect, NDataTable, NTag, NEmpty, NSpin,
  NInput, NButton, NModal,
  type DataTableColumns,
} from 'naive-ui'
import { listTags, searchTags, getTagCooccurrences } from '@/api/client'
import type { TagResponse, TagMatchResponse, CoocEntry, CooccurrenceResponse, ForceGraphNode, ForceGraphLink } from '@/types'
import ForceGraph from '@/components/graph/ForceGraph.vue'

const agents = ['coder', 'researcher', 'writer']
const selectedAgent = ref('coder')
const loading = ref(false)
const tags = ref<TagResponse[]>([])

// Search
const searchQuery = ref('')
const searching = ref(false)
const searchResults = ref<TagMatchResponse[]>([])

// Cooccurrence modal
const showCooc = ref(false)
const coocLoading = ref(false)
const coocData = ref<CooccurrenceResponse | null>(null)
const selectedTagLabel = ref('')
const selectedTagId = ref('')

// Graph visualization
const graphTag = ref<TagResponse | null>(null)
const graphCoocData = ref<CooccurrenceResponse | null>(null)
const graphLoading = ref(false)

const graphNodes = computed<ForceGraphNode[]>(() => {
  if (!graphTag.value || !graphCoocData.value) return []
  const center: ForceGraphNode = {
    id: graphTag.value.id,
    label: graphTag.value.label,
    group: 'center',
    size: Math.max(6, Math.min(14, graphTag.value.frequency * 0.5)),
  }
  const neighbors: ForceGraphNode[] = graphCoocData.value.cooccurrences.map((c) => ({
    id: c.partner_tag.id,
    label: c.partner_tag.label,
    group: 'tag',
    size: Math.max(4, Math.min(10, c.partner_tag.frequency * 0.4)),
  }))
  return [center, ...neighbors]
})

const graphLinks = computed<ForceGraphLink[]>(() => {
  if (!graphTag.value || !graphCoocData.value) return []
  // Normalize PMI for link weight: map PMI range to 0.5..3
  const pmis = graphCoocData.value.cooccurrences.map((c) => c.pmi)
  const maxPmi = Math.max(...pmis, 1)
  return graphCoocData.value.cooccurrences.map((c) => ({
    source: graphTag.value!.id,
    target: c.partner_tag.id,
    weight: 0.5 + (c.pmi / maxPmi) * 2.5,
  }))
})

async function loadGraphForTag(tag: TagResponse) {
  graphTag.value = tag
  graphLoading.value = true
  try {
    graphCoocData.value = await getTagCooccurrences(tag.id)
  } catch {
    graphCoocData.value = null
  } finally {
    graphLoading.value = false
  }
}

function handleGraphNodeClick(node: ForceGraphNode) {
  // If clicking a neighbor tag, re-center the graph on that tag
  if (node.id === graphTag.value?.id) return
  const matchingTag = tags.value.find((t) => t.id === node.id)
  // Also check co-occurrence partner tags
  const partnerTag = graphCoocData.value?.cooccurrences.find((c) => c.partner_tag.id === node.id)?.partner_tag
  const tag = matchingTag ?? (partnerTag ? { ...partnerTag } : null)
  if (tag) loadGraphForTag(tag)
}

const columns: DataTableColumns<TagResponse> = [
  { title: '标签名', key: 'label', sorter: 'default' },
  { title: '标准形式', key: 'canonical', width: 200 },
  {
    title: '频次',
    key: 'frequency',
    width: 100,
    sorter: (a, b) => a.frequency - b.frequency,
  },
  {
    title: '重要度',
    key: 'importance',
    width: 110,
    render: (row) => row.importance.toFixed(3),
    sorter: (a, b) => a.importance - b.importance,
  },
  {
    title: '操作',
    key: 'actions',
    width: 260,
    render: (row) =>
      h(NSpace, { size: 8 }, () => [
        h(
          NButton,
          { size: 'small', type: 'info', onClick: () => showCooccurrences(row) },
          () => '共现标签',
        ),
        h(
          NButton,
          { size: 'small', type: 'success', onClick: () => loadGraphForTag(row) },
          () => '图谱',
        ),
      ]),
  },
]

const searchColumns: DataTableColumns<TagMatchResponse> = [
  { title: '标签名', key: 'tag.label', render: (row) => row.tag.label },
  {
    title: '相似度',
    key: 'similarity',
    width: 110,
    render: (row) => row.similarity.toFixed(4),
  },
  {
    title: '频次',
    key: 'tag.frequency',
    width: 100,
    render: (row) => String(row.tag.frequency),
  },
  {
    title: '重要度',
    key: 'tag.importance',
    width: 110,
    render: (row) => row.tag.importance.toFixed(3),
  },
]

const coocColumns: DataTableColumns<CoocEntry> = [
  { title: '关联标签', key: 'partner_tag.label', render: (row) => row.partner_tag.label },
  {
    title: '共现次数',
    key: 'count',
    width: 80,
    render: (row) => String(row.count),
  },
  {
    title: 'PMI分数',
    key: 'pmi',
    width: 100,
    render: (row) => row.pmi.toFixed(3),
  },
]

async function refresh() {
  loading.value = true
  try {
    const res = await listTags(selectedAgent.value)
    tags.value = res.tags
  } catch (e) {
    tags.value = []
  } finally {
    loading.value = false
  }
}

async function handleSearch() {
  if (!searchQuery.value.trim()) return
  searching.value = true
  try {
    const res = await searchTags(searchQuery.value, selectedAgent.value)
    searchResults.value = res.results
  } finally {
    searching.value = false
  }
}

async function showCooccurrences(tag: TagResponse) {
  selectedTagLabel.value = tag.label
  selectedTagId.value = tag.id
  showCooc.value = true
  coocLoading.value = true
  try {
    coocData.value = await getTagCooccurrences(tag.id)
  } finally {
    coocLoading.value = false
  }
}

watch(selectedAgent, () => {
  graphTag.value = null
  graphCoocData.value = null
  refresh()
}, { immediate: true })
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">标签管理</h2>
      <NSelect
        v-model:value="selectedAgent"
        :options="agents.map(a => ({ label: a, value: a }))"
        style="width: 160px"
        size="small"
      />
      <NTag type="info" size="small">{{ tags.length }} 个标签</NTag>
    </NSpace>

    <!-- Tag Co-occurrence Graph -->
    <NCard title="标签共现图" size="small">
      <template #header-extra>
        <NTag v-if="graphTag" type="success" size="small" closable @close="graphTag = null; graphCoocData = null">
          {{ graphTag.label }}
        </NTag>
      </template>
      <NSpin :show="graphLoading">
        <div v-if="graphNodes.length > 0">
          <p style="margin: 0 0 8px; color: #8b949e; font-size: 12px">
            在下方表格中点击标签的「图谱」按钮以可视化其共现网络。
            点击邻居节点可重新定位中心。
          </p>
          <ForceGraph
            :nodes="graphNodes"
            :links="graphLinks"
            :width="780"
            :height="420"
            :center-node-id="graphTag?.id"
            @node-click="handleGraphNodeClick"
          />
        </div>
        <NEmpty v-else description="从下方表格选择标签并点击「图谱」以可视化共现关系。" />
      </NSpin>
    </NCard>

    <!-- Semantic Search -->
    <NCard title="按语义搜索标签" size="small">
      <NSpace :size="12">
        <NInput
          v-model:value="searchQuery"
          placeholder="按语义相似度搜索标签..."
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
          搜索
        </NButton>
      </NSpace>

      <div v-if="searchResults.length > 0" style="margin-top: 12px">
        <NDataTable
          :columns="searchColumns"
          :data="searchResults"
          :row-key="(row: TagMatchResponse) => row.tag.id"
          :max-height="300"
          striped
          size="small"
        />
      </div>
    </NCard>

    <!-- All Tags Table -->
    <NCard title="全部标签" size="small">
      <NSpin :show="loading">
        <NDataTable
          v-if="tags.length > 0"
          :columns="columns"
          :data="tags"
          :row-key="(row: TagResponse) => row.id"
          :max-height="500"
          striped
          size="small"
          :pagination="{ pageSize: 25 }"
        />
        <NEmpty v-else description="未找到标签。请先摄入文档以自动提取标签。" />
      </NSpin>
    </NCard>

    <!-- Cooccurrence Modal -->
    <NModal
      v-model:show="showCooc"
      preset="card"
      :title="`共现标签: ${selectedTagLabel}`"
      style="width: 600px"
    >
      <NSpin :show="coocLoading">
        <NDataTable
          v-if="coocData && coocData.cooccurrences.length > 0"
          :columns="coocColumns"
          :data="coocData.cooccurrences"
          :row-key="(row: CoocEntry) => row.partner_tag.id"
          :max-height="400"
          striped
          size="small"
        />
        <NEmpty v-else description="未找到共现标签。" />
      </NSpin>
    </NModal>
  </NSpace>
</template>
