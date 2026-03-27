<script setup lang="ts">
import { ref } from 'vue'
import {
  NCard, NSpace, NSelect, NRadioGroup, NRadioButton,
  NInput, NButton, NTag, NAlert,
  NStatistic, NGrid, NGi, NCollapse, NCollapseItem,
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
        <!-- Tags 由系统自动抽取，不再需要手动输入 -->
        <!-- <div>
          <div style="color: #999; font-size: 12px; margin-bottom: 4px">Tags (comma-separated)</div>
          <NInput
            v-model:value="tagsInput"
            placeholder="e.g. rust, async, tutorial"
            style="width: 250px"
            size="small"
          />
        </div> -->
      </NSpace>
    </NCard>

    <!-- Input -->
    <NCard title="Document Text" size="small">
      <NInput
        v-model:value="documentText"
        type="textarea"
        placeholder="Paste your document text here..."
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
    <template v-if="result">
      <!-- Stats -->
      <NCard title="Ingestion Result" size="small">
        <NGrid :cols="5" :x-gap="12">
          <NGi><NStatistic label="Chunks Created" :value="result.chunks_created" /></NGi>
          <NGi><NStatistic label="Chunks Stored" :value="result.chunks_stored" /></NGi>
          <NGi>
            <NStatistic label="Total Time">
              <template #default>{{ result.total_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
          <NGi>
            <NStatistic label="Encode Time">
              <template #default>{{ result.latency.encode_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
          <NGi>
            <NStatistic label="Store Time">
              <template #default>{{ result.latency.store_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>

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

      <!-- Document Skeleton -->
      <NCard title="Document Skeleton" size="small">
        <NSpace :size="8" vertical>
          <div>
            <NTag type="primary" size="small">Title</NTag>
            <span style="color: #e6edf3; margin-left: 8px">{{ result.title }}</span>
          </div>
          <div>
            <NTag type="info" size="small">Summary</NTag>
            <span style="color: #ccc; margin-left: 8px; font-size: 13px">{{ result.summary }}</span>
          </div>
        </NSpace>
      </NCard>

      <!-- Chunk Details -->
      <NCard title="Chunk Visualization" size="small">
        <div style="color: #999; font-size: 12px; margin-bottom: 8px">
          {{ result.chunks.length }} chunks | Click to expand context details
        </div>
        <NCollapse>
          <NCollapseItem
            v-for="chunk in result.chunks"
            :key="chunk.index"
            :title="`Chunk ${chunk.index} — ${chunk.section} (${chunk.char_count} chars)`"
            :name="chunk.index"
          >
            <template #header-extra>
              <NTag size="tiny" type="default" style="margin-right: 4px">
                {{ chunk.memory_id.slice(0, 8) }}...
              </NTag>
            </template>

            <!-- Context prefix -->
            <div style="margin-bottom: 8px">
              <div style="color: #f0a020; font-size: 11px; font-weight: bold; margin-bottom: 4px">
                Context Prefix (injected)
              </div>
              <div style="
                background: #1a1a2e;
                border-left: 3px solid #f0a020;
                padding: 8px 12px;
                font-size: 12px;
                color: #f0a020;
                font-family: monospace;
                white-space: pre-wrap;
                word-break: break-all;
              ">{{ chunk.context_prefix }}</div>
            </div>

            <!-- Original text -->
            <div style="margin-bottom: 8px">
              <div style="color: #18a058; font-size: 11px; font-weight: bold; margin-bottom: 4px">
                Original Text
              </div>
              <div style="
                background: #0d1117;
                border-left: 3px solid #18a058;
                padding: 8px 12px;
                font-size: 13px;
                color: #e6edf3;
                white-space: pre-wrap;
                word-break: break-all;
                max-height: 200px;
                overflow-y: auto;
              ">{{ chunk.original_text }}</div>
            </div>

            <!-- Tags -->
            <NSpace :size="4">
              <NTag v-for="tag in chunk.tags" :key="tag" size="tiny" type="default">
                {{ tag }}
              </NTag>
            </NSpace>
          </NCollapseItem>
        </NCollapse>
      </NCard>
    </template>
  </NSpace>
</template>
