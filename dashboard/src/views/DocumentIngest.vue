<script setup lang="ts">
import { ref } from 'vue'
import {
  NCard, NSpace, NSelect, NRadioGroup, NRadioButton,
  NInput, NButton, NTag, NAlert, NSpin,
  NStatistic, NGrid, NGi,
} from 'naive-ui'
import { ingestDocument } from '@/api/client'
import type { IngestResponse } from '@/types'

const agents = ['coder', 'researcher', 'writer']
const selectedAgent = ref('coder')
const selectedScope = ref<'private' | 'shared'>('private')
const tagsInput = ref('')
const documentText = ref('')
const ingesting = ref(false)
const result = ref<IngestResponse | null>(null)
const error = ref<string | null>(null)

async function handleIngest() {
  if (!documentText.value.trim()) return
  ingesting.value = true
  error.value = null
  result.value = null

  try {
    const tags = tagsInput.value
      .split(',')
      .map(t => t.trim())
      .filter(t => t.length > 0)

    result.value = await ingestDocument(
      documentText.value,
      selectedAgent.value,
      selectedScope.value,
      tags,
    )
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    ingesting.value = false
  }
}

function clearAll() {
  documentText.value = ''
  result.value = null
  error.value = null
  tagsInput.value = ''
}
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">Document Ingest</h2>
      <NTag type="info" size="small">
        Paste text below to chunk, encode, and store as memories
      </NTag>
    </NSpace>

    <!-- Config -->
    <NCard title="Configuration" size="small">
      <NSpace :size="16" align="center">
        <div>
          <div style="color: #999; font-size: 12px; margin-bottom: 4px">Agent</div>
          <NSelect
            v-model:value="selectedAgent"
            :options="agents.map(a => ({ label: a, value: a }))"
            style="width: 150px"
            size="small"
          />
        </div>
        <div>
          <div style="color: #999; font-size: 12px; margin-bottom: 4px">Scope</div>
          <NRadioGroup v-model:value="selectedScope" size="small">
            <NRadioButton value="private">Private</NRadioButton>
            <NRadioButton value="shared">Shared</NRadioButton>
          </NRadioGroup>
        </div>
        <div>
          <div style="color: #999; font-size: 12px; margin-bottom: 4px">Tags (comma-separated)</div>
          <NInput
            v-model:value="tagsInput"
            placeholder="e.g. rust, async, tutorial"
            style="width: 250px"
            size="small"
          />
        </div>
      </NSpace>
    </NCard>

    <!-- Input -->
    <NCard title="Document Text" size="small">
      <NInput
        v-model:value="documentText"
        type="textarea"
        placeholder="Paste your document text here...&#10;&#10;The text will be automatically chunked, contextualized, encoded via Gemini, and stored in the memory system."
        :rows="12"
        :disabled="ingesting"
      />
      <NSpace style="margin-top: 12px" :size="12">
        <NButton
          type="primary"
          :loading="ingesting"
          :disabled="!documentText.trim()"
          @click="handleIngest"
        >
          Ingest Document
        </NButton>
        <NButton @click="clearAll" :disabled="ingesting">Clear</NButton>
        <NTag v-if="documentText" size="small" type="default">
          {{ documentText.length.toLocaleString() }} chars
        </NTag>
      </NSpace>
    </NCard>

    <!-- Error -->
    <NAlert v-if="error" type="error" title="Ingestion Failed" closable @close="error = null">
      {{ error }}
    </NAlert>

    <!-- Result -->
    <NCard v-if="result" title="Ingestion Result" size="small">
      <NGrid :cols="5" :x-gap="12">
        <NGi>
          <NStatistic label="Chunks Created" :value="result.chunks_created" />
        </NGi>
        <NGi>
          <NStatistic label="Chunks Stored" :value="result.chunks_stored" />
        </NGi>
        <NGi>
          <NStatistic label="Total Time">
            <template #default>
              {{ result.total_ms }}
              <span style="font-size: 12px; color: #999">ms</span>
            </template>
          </NStatistic>
        </NGi>
        <NGi>
          <NStatistic label="Encode Time">
            <template #default>
              {{ result.latency.encode_ms }}
              <span style="font-size: 12px; color: #999">ms</span>
            </template>
          </NStatistic>
        </NGi>
        <NGi>
          <NStatistic label="Store Time">
            <template #default>
              {{ result.latency.store_ms }}
              <span style="font-size: 12px; color: #999">ms</span>
            </template>
          </NStatistic>
        </NGi>
      </NGrid>

      <div style="margin-top: 12px">
        <NTag type="success" size="small">Title: {{ result.title }}</NTag>
      </div>

      <!-- Pipeline Breakdown -->
      <div style="display: flex; gap: 8px; margin-top: 16px; align-items: stretch">
        <NCard size="small" style="flex: 1; text-align: center">
          <div style="color: #999; font-size: 11px">Chunk</div>
          <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ result.latency.chunk_ms }}ms</div>
        </NCard>
        <div style="display: flex; align-items: center; color: #555">&rarr;</div>
        <NCard size="small" style="flex: 1; text-align: center">
          <div style="color: #999; font-size: 11px">Skeleton</div>
          <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ result.latency.skeleton_ms }}ms</div>
        </NCard>
        <div style="display: flex; align-items: center; color: #555">&rarr;</div>
        <NCard size="small" style="flex: 1; text-align: center">
          <div style="color: #999; font-size: 11px">Encode</div>
          <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ result.latency.encode_ms }}ms</div>
        </NCard>
        <div style="display: flex; align-items: center; color: #555">&rarr;</div>
        <NCard size="small" style="flex: 1; text-align: center; border-color: #18a058">
          <div style="color: #18a058; font-size: 11px">Store</div>
          <div style="color: #18a058; font-size: 16px; font-weight: bold">{{ result.latency.store_ms }}ms</div>
          <div style="color: #666; font-size: 11px">{{ result.chunks_stored }} entries</div>
        </NCard>
      </div>
    </NCard>
  </NSpace>
</template>
