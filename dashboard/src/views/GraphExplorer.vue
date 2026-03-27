<script setup lang="ts">
import { ref, computed } from 'vue'
import {
  NCard, NSpace, NInput, NButton, NEmpty, NTag, NDataTable, NSpin,
  NInputNumber, NSelect,
  type DataTableColumns,
} from 'naive-ui'
import { searchGraph, getNodeDetail, traverseGraph } from '@/api/client'
import type { KgNode, KgEdge, ForceGraphNode, ForceGraphLink } from '@/types'
import ForceGraph from '@/components/graph/ForceGraph.vue'

const searchQuery = ref('')
const nodes = ref<KgNode[]>([])
const selectedNode = ref<KgNode | null>(null)
const selectedEdges = ref<KgEdge[]>([])
const searching = ref(false)

// Traversal / graph visualization
const traversalHops = ref(2)
const traversalLoading = ref(false)
const traversalNodes = ref<KgNode[]>([])
const traversalEdges = ref<KgEdge[]>([])
const traversalCenterId = ref('')

const NODE_TYPE_COLORS: Record<string, string> = {
  Entity: 'Entity',
  Concept: 'Concept',
  Relation: 'Relation',
}

const graphNodes = computed<ForceGraphNode[]>(() => {
  return traversalNodes.value.map((n) => ({
    id: n.id,
    label: n.label,
    group: NODE_TYPE_COLORS[n.node_type] ?? 'default',
    size: n.id === traversalCenterId.value
      ? 10
      : Math.max(4, Math.min(10, n.importance * 8)),
  }))
})

const graphLinks = computed<ForceGraphLink[]>(() => {
  // Only include edges where both endpoints exist in traversalNodes
  const nodeIds = new Set(traversalNodes.value.map((n) => n.id))
  return traversalEdges.value
    .filter((e) => nodeIds.has(e.source_id) && nodeIds.has(e.target_id))
    .map((e) => ({
      source: e.source_id,
      target: e.target_id,
      weight: e.weight,
      label: e.relation,
    }))
})

const nodeColumns: DataTableColumns<KgNode> = [
  { title: 'Label', key: 'label' },
  { title: 'Type', key: 'node_type', width: 100 },
  { title: 'Importance', key: 'importance', width: 100, render: (row) => row.importance.toFixed(2) },
  { title: 'Scope', key: 'agent_id', width: 100, render: (row) => row.agent_id ?? 'shared' },
  {
    title: 'Actions',
    key: 'actions',
    width: 120,
    render: (row) =>
      h(
        NButton,
        { size: 'small', type: 'success', onClick: () => loadTraversal(row) },
        () => 'Traverse',
      ),
  },
]

const edgeColumns: DataTableColumns<KgEdge> = [
  { title: 'Relation', key: 'relation' },
  { title: 'Source', key: 'source_id', width: 100, render: (row) => row.source_id.slice(0, 8) },
  { title: 'Target', key: 'target_id', width: 100, render: (row) => row.target_id.slice(0, 8) },
  { title: 'Weight', key: 'weight', width: 80 },
]

import { h } from 'vue'

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

async function loadTraversal(node: KgNode) {
  traversalCenterId.value = node.id
  traversalLoading.value = true
  try {
    const res = await traverseGraph(node.id, traversalHops.value)
    traversalNodes.value = res.nodes
    traversalEdges.value = res.edges
  } catch {
    traversalNodes.value = []
    traversalEdges.value = []
  } finally {
    traversalLoading.value = false
  }
}

function handleGraphNodeClick(node: ForceGraphNode) {
  if (node.id === traversalCenterId.value) return
  // Find the KgNode and re-center traversal
  const kgNode = traversalNodes.value.find((n) => n.id === node.id)
  if (kgNode) loadTraversal(kgNode)
}
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

    <!-- Entity Relationship Graph -->
    <NCard title="Entity Relationship Graph" size="small">
      <template #header-extra>
        <NSpace :size="8" align="center">
          <span style="color: #8b949e; font-size: 12px">Hops:</span>
          <NInputNumber
            v-model:value="traversalHops"
            :min="1"
            :max="5"
            size="small"
            style="width: 80px"
          />
          <NTag v-if="traversalCenterId" type="success" size="small" closable @close="traversalNodes = []; traversalEdges = []; traversalCenterId = ''">
            {{ traversalNodes.find(n => n.id === traversalCenterId)?.label ?? traversalCenterId.slice(0, 8) }}
          </NTag>
        </NSpace>
      </template>
      <NSpin :show="traversalLoading">
        <div v-if="graphNodes.length > 0">
          <p style="margin: 0 0 8px; color: #8b949e; font-size: 12px">
            {{ traversalNodes.length }} nodes, {{ graphLinks.length }} edges.
            Click a node to re-center the traversal. Drag nodes to rearrange.
          </p>
          <NSpace :size="8" style="margin-bottom: 8px">
            <NTag :bordered="false" style="background: rgba(24,160,88,0.2); color: #18a058" size="small">Entity</NTag>
            <NTag :bordered="false" style="background: rgba(88,166,255,0.2); color: #58a6ff" size="small">Concept</NTag>
            <NTag :bordered="false" style="background: rgba(240,160,32,0.2); color: #f0a020" size="small">Relation</NTag>
            <NTag :bordered="false" style="background: rgba(249,115,22,0.2); color: #f97316" size="small">Center</NTag>
          </NSpace>
          <ForceGraph
            :nodes="graphNodes"
            :links="graphLinks"
            :width="780"
            :height="500"
            :center-node-id="traversalCenterId"
            @node-click="handleGraphNodeClick"
          />
        </div>
        <NEmpty v-else description="Search for a node above, then click 'Traverse' to visualize the graph neighborhood." />
      </NSpin>
    </NCard>

    <!-- Search Results Table -->
    <NCard title="Nodes" size="small">
      <NDataTable
        v-if="nodes.length > 0"
        :columns="nodeColumns"
        :data="nodes"
        :row-key="(row: KgNode) => row.id"
        :max-height="300"
        striped
        size="small"
        :row-props="(row: KgNode) => ({ style: 'cursor: pointer', onClick: () => handleNodeClick(row) })"
      />
      <NEmpty v-else description="Search for nodes or seed demo data first." />
    </NCard>

    <!-- Node Detail Panel -->
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
