<script setup lang="ts">
import { ref } from 'vue'
import {
  NCard, NSpace,
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
      <h2 style="margin: 0; color: #e6edf3">文档摄入</h2>
    </NSpace>

    <!-- Input -->
    <NCard title="文档文本" size="small">
      <NInput
        v-model:value="documentText"
        type="textarea"
        placeholder="在此粘贴文档文本..."
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
          摄入文档
        </NButton>
        <NButton @click="clearAll" :disabled="ingesting">清空</NButton>
        <NTag v-if="documentText" size="small" type="default">
          {{ documentText.length.toLocaleString() }} 字符
        </NTag>
      </NSpace>
    </NCard>

    <!-- Error -->
    <NAlert v-if="error" type="error" title="摄入失败" closable @close="error = null">
      {{ error }}
    </NAlert>

    <!-- Result -->
    <template v-if="result">
      <!-- Stats -->
      <NCard title="摄入结果" size="small">
        <NGrid :cols="5" :x-gap="12">
          <NGi><NStatistic label="创建分块数" :value="result.chunks_created" /></NGi>
          <NGi><NStatistic label="存储分块数" :value="result.chunks_stored" /></NGi>
          <NGi>
            <NStatistic label="总耗时">
              <template #default>{{ result.total_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
          <NGi>
            <NStatistic label="编码耗时">
              <template #default>{{ result.latency.encode_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
          <NGi>
            <NStatistic label="存储耗时">
              <template #default>{{ result.latency.store_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>

        <!-- Pipeline Breakdown -->
        <div style="display: flex; gap: 8px; margin-top: 16px; align-items: stretch">
          <NCard size="small" style="flex: 1; text-align: center">
            <div style="color: #999; font-size: 11px">分块</div>
            <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ result.latency.chunk_ms }}ms</div>
          </NCard>
          <div style="display: flex; align-items: center; color: #555">&rarr;</div>
          <NCard size="small" style="flex: 1; text-align: center">
            <div style="color: #999; font-size: 11px">骨架</div>
            <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ result.latency.skeleton_ms }}ms</div>
          </NCard>
          <div style="display: flex; align-items: center; color: #555">&rarr;</div>
          <NCard size="small" style="flex: 1; text-align: center">
            <div style="color: #999; font-size: 11px">编码</div>
            <div style="color: #e6edf3; font-size: 16px; font-weight: bold">{{ result.latency.encode_ms }}ms</div>
          </NCard>
          <div style="display: flex; align-items: center; color: #555">&rarr;</div>
          <NCard size="small" style="flex: 1; text-align: center; border-color: #18a058">
            <div style="color: #18a058; font-size: 11px">存储</div>
            <div style="color: #18a058; font-size: 16px; font-weight: bold">{{ result.latency.store_ms }}ms</div>
            <div style="color: #666; font-size: 11px">{{ result.chunks_stored }} 条</div>
          </NCard>
        </div>
      </NCard>

      <!-- Document Skeleton -->
      <NCard title="文档骨架" size="small">
        <NSpace :size="8" vertical>
          <div>
            <NTag type="primary" size="small">标题</NTag>
            <span style="color: #e6edf3; margin-left: 8px">{{ result.title }}</span>
          </div>
          <div>
            <NTag type="info" size="small">摘要</NTag>
            <span style="color: #ccc; margin-left: 8px; font-size: 13px">{{ result.summary }}</span>
          </div>
        </NSpace>
      </NCard>

      <!-- Chunk Details -->
      <NCard title="分块可视化" size="small">
        <div style="color: #999; font-size: 12px; margin-bottom: 8px">
          {{ result.chunks.length }} 个分块 | 点击展开上下文详情
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
                上下文前缀（注入）
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
                原始文本
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
