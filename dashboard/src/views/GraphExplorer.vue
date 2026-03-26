<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NCard, NSpace, NInput, NButton, NEmpty, NTag, NDataTable, type DataTableColumns } from 'naive-ui'
import { searchGraph, getNodeDetail } from '@/api/client'
import type { KgNode, KgEdge } from '@/types'

const searchQuery = ref('')
const nodes = ref<KgNode[]>([])
const selectedNode = ref<KgNode | null>(null)
const selectedEdges = ref<KgEdge[]>([])
const searching = ref(false)

const nodeColumns: DataTableColumns<KgNode> = [
  { title: 'Label', key: 'label' },
  { title: 'Type', key: 'node_type', width: 100 },
  { title: 'Importance', key: 'importance', width: 100, render: (row) => row.importance.toFixed(2) },
  { title: 'Scope', key: 'agent_id', width: 100, render: (row) => row.agent_id ?? 'shared' },
]

const edgeColumns: DataTableColumns<KgEdge> = [
  { title: 'Relation', key: 'relation' },
  { title: 'Source', key: 'source_id', width: 100, render: (row) => row.source_id.slice(0, 8) },
  { title: 'Target', key: 'target_id', width: 100, render: (row) => row.target_id.slice(0, 8) },
  { title: 'Weight', key: 'weight', width: 80 },
]

async function handleSearch() {
  if (!searchQuery.value.trim()) return
  searching.value = true
  try {
    const res = await searchGraph(searchQuery.value)
    nodes.value = res.nodes
    selectedNode.value = null
    selectedEdges.value = []
  } finally {
    searching.value = false
  }
}

async function handleNodeClick(row: KgNode) {
  const res = await getNodeDetail(row.id)
  selectedNode.value = res.node
  selectedEdges.value = res.edges
}

onMounted(() => {
  // Default search to load all shared nodes
  searchQuery.value = ''
})
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">Knowledge Graph</h2>
      <NInput
        v-model:value="searchQuery"
        placeholder="Search nodes by label..."
        style="width: 300px"
        size="small"
        @keyup.enter="handleSearch"
      />
      <NButton @click="handleSearch" :loading="searching" type="primary" ghost size="small">
        Search
      </NButton>
    </NSpace>

    <NCard title="Nodes" size="small">
      <NDataTable
        v-if="nodes.length > 0"
        :columns="nodeColumns"
        :data="nodes"
        :row-key="(row: KgNode) => row.id"
        :max-height="300"
        striped
        size="small"
        @update:checked-row-keys="() => {}"
        :row-props="(row: KgNode) => ({ style: 'cursor: pointer', onClick: () => handleNodeClick(row) })"
      />
      <NEmpty v-else description="Search for nodes or seed demo data first." />
    </NCard>

    <NCard v-if="selectedNode" :title="`Node: ${selectedNode.label}`" size="small">
      <NSpace vertical :size="8">
        <NSpace>
          <NTag type="info">{{ selectedNode.node_type }}</NTag>
          <NTag>importance: {{ selectedNode.importance.toFixed(2) }}</NTag>
          <NTag>{{ selectedNode.agent_id ?? 'shared' }}</NTag>
        </NSpace>
        <div v-if="selectedEdges.length > 0">
          <h4 style="color: #e6edf3">Edges ({{ selectedEdges.length }})</h4>
          <NDataTable
            :columns="edgeColumns"
            :data="selectedEdges"
            :row-key="(row: KgEdge) => row.id"
            striped
            size="small"
          />
        </div>
      </NSpace>
    </NCard>
  </NSpace>
</template>
