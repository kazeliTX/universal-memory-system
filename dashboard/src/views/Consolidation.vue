<script setup lang="ts">
import { ref } from 'vue'
import {
  NCard, NSpace, NSelect, NButton, NAlert,
  NStatistic, NGrid, NGi, NTag,
} from 'naive-ui'
import { runConsolidation } from '@/api/client'
import type { ConsolidationReportResponse } from '@/types'

const agents = ['coder', 'researcher', 'writer']
const selectedAgent = ref('coder')
const running = ref(false)
const result = ref<ConsolidationReportResponse | null>(null)
const error = ref<string | null>(null)

async function handleRun() {
  running.value = true
  error.value = null
  result.value = null

  try {
    result.value = await runConsolidation(selectedAgent.value)
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    running.value = false
  }
}
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" :size="16">
      <h2 style="margin: 0; color: #e6edf3">记忆巩固 (M4)</h2>
    </NSpace>

    <!-- Controls -->
    <NCard title="运行巩固" size="small">
      <NSpace :size="16" align="center">
        <div>
          <div style="color: #999; font-size: 12px; margin-bottom: 4px">智能体</div>
          <NSelect
            v-model:value="selectedAgent"
            :options="agents.map(a => ({ label: a, value: a }))"
            style="width: 180px"
            size="small"
          />
        </div>
        <NButton
          type="primary"
          :loading="running"
          @click="handleRun"
          style="margin-top: 16px"
        >
          运行巩固
        </NButton>
      </NSpace>
      <div style="color: #666; font-size: 12px; margin-top: 12px">
        触发完整巩固周期：记忆衰减、图谱演化和自动提升。
      </div>
    </NCard>

    <!-- Error -->
    <NAlert v-if="error" type="error" title="巩固失败" closable @close="error = null">
      {{ error }}
    </NAlert>

    <!-- Results -->
    <template v-if="result">
      <!-- Summary -->
      <NCard title="巩固报告" size="small">
        <NSpace :size="8" style="margin-bottom: 12px">
          <NTag type="primary" size="small">{{ result.agent_id }}</NTag>
          <NTag type="default" size="small">{{ result.timestamp }}</NTag>
        </NSpace>
        <NGrid :cols="2" :x-gap="12">
          <NGi>
            <NStatistic label="总耗时">
              <template #default>{{ result.total_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>

      <!-- Decay -->
      <NCard title="记忆衰减" size="small">
        <NGrid :cols="4" :x-gap="12">
          <NGi><NStatistic label="已扫描" :value="result.decay.scanned" /></NGi>
          <NGi><NStatistic label="已更新" :value="result.decay.updated" /></NGi>
          <NGi><NStatistic label="已归档" :value="result.decay.archived" /></NGi>
          <NGi>
            <NStatistic label="耗时">
              <template #default>{{ result.decay.elapsed_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>

      <!-- Graph Evolution -->
      <NCard title="图谱演化" size="small">
        <NGrid :cols="4" :x-gap="12">
          <NGi><NStatistic label="扫描节点对" :value="result.evolution.pairs_scanned" /></NGi>
          <NGi><NStatistic label="合并节点" :value="result.evolution.nodes_merged" /></NGi>
          <NGi><NStatistic label="加强边" :value="result.evolution.edges_strengthened" /></NGi>
          <NGi>
            <NStatistic label="耗时">
              <template #default>{{ result.evolution.elapsed_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>

      <!-- Auto Promote -->
      <NCard title="自动提升" size="small">
        <NGrid :cols="3" :x-gap="12">
          <NGi><NStatistic label="已扫描" :value="result.promotion.scanned" /></NGi>
          <NGi><NStatistic label="已提升" :value="result.promotion.promoted" /></NGi>
          <NGi>
            <NStatistic label="耗时">
              <template #default>{{ result.promotion.elapsed_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>
    </template>
  </NSpace>
</template>
