<script setup lang="ts">
import { ref, watch, h } from 'vue'
import {
  NCard, NSpace, NSelect, NDataTable, NTag, NEmpty, NSpin,
  NInput, NButton, NModal, NDescriptions, NDescriptionsItem,
  type DataTableColumns,
} from 'naive-ui'
import { listTags, searchTags, getTagCooccurrences } from '@/api/client'
import type { TagResponse, TagMatchResponse, CoocEntry, CooccurrenceResponse } from '@/types'

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

const columns: DataTableColumns<TagResponse> = [
  { title: 'Label', key: 'label', sorter: 'default' },
  { title: 'Canonical', key: 'canonical', width: 200 },
  {
    title: 'Frequency',
    key: 'frequency',
    width: 100,
    sorter: (a, b) => a.frequency - b.frequency,
  },
  {
    title: 'Importance',
    key: 'importance',
    width: 110,
    render: (row) => row.importance.toFixed(3),
    sorter: (a, b) => a.importance - b.importance,
  },
  {
    title: 'Actions',
    key: 'actions',
    width: 140,
    render: (row) =>
      h(
        NButton,
        {
          size: 'small',
          type: 'info',
          onClick: () => showCooccurrences(row),
        },
        () => 'Co-occurrences',
      ),
  },
]

const searchColumns: DataTableColumns<TagMatchResponse> = [
  { title: 'Label', key: 'tag.label', render: (row) => row.tag.label },
  {
    title: 'Similarity',
    key: 'similarity',
    width: 110,
    render: (row) => row.similarity.toFixed(4),
  },
  {
    title: 'Frequency',
    key: 'tag.frequency',
    width: 100,
    render: (row) => String(row.tag.frequency),
  },
  {
    title: 'Importance',
    key: 'tag.importance',
    width: 110,
    render: (row) => row.tag.importance.toFixed(3),
  },
]

const coocColumns: DataTableColumns<CoocEntry> = [
  { title: 'Partner Tag', key: 'partner_tag.label', render: (row) => row.partner_tag.label },
  {
    title: 'Count',
    key: 'count',
    width: 80,
    render: (row) => String(row.count),
  },
  {
    title: 'PMI',
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
  showCooc.value = true
  coocLoading.value = true
  try {
    coocData.value = await getTagCooccurrences(tag.id)
  } finally {
    coocLoading.value = false
  }
}

watch(selectedAgent, refresh, { immediate: true })
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">Tag Explorer</h2>
      <NSelect
        v-model:value="selectedAgent"
        :options="agents.map(a => ({ label: a, value: a }))"
        style="width: 160px"
        size="small"
      />
      <NTag type="info" size="small">{{ tags.length }} tags</NTag>
    </NSpace>

    <!-- Semantic Search -->
    <NCard title="Search Tags by Meaning" size="small">
      <NSpace :size="12">
        <NInput
          v-model:value="searchQuery"
          placeholder="Search tags by semantic similarity..."
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
    <NCard title="All Tags" size="small">
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
        <NEmpty v-else description="No tags found. Ingest documents to auto-extract tags." />
      </NSpin>
    </NCard>

    <!-- Cooccurrence Modal -->
    <NModal
      v-model:show="showCooc"
      preset="card"
      :title="`Co-occurrences: ${selectedTagLabel}`"
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
        <NEmpty v-else description="No co-occurring tags found." />
      </NSpin>
    </NModal>
  </NSpace>
</template>
