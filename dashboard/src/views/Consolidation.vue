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
      <h2 style="margin: 0; color: #e6edf3">Consolidation (M4)</h2>
    </NSpace>

    <!-- Controls -->
    <NCard title="Run Consolidation" size="small">
      <NSpace :size="16" align="center">
        <div>
          <div style="color: #999; font-size: 12px; margin-bottom: 4px">Agent</div>
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
          Run Consolidation
        </NButton>
      </NSpace>
      <div style="color: #666; font-size: 12px; margin-top: 12px">
        Triggers a full consolidation cycle: memory decay, graph evolution, and auto-promotion.
      </div>
    </NCard>

    <!-- Error -->
    <NAlert v-if="error" type="error" title="Consolidation Failed" closable @close="error = null">
      {{ error }}
    </NAlert>

    <!-- Results -->
    <template v-if="result">
      <!-- Summary -->
      <NCard title="Consolidation Report" size="small">
        <NSpace :size="8" style="margin-bottom: 12px">
          <NTag type="primary" size="small">{{ result.agent_id }}</NTag>
          <NTag type="default" size="small">{{ result.timestamp }}</NTag>
        </NSpace>
        <NGrid :cols="2" :x-gap="12">
          <NGi>
            <NStatistic label="Total Time">
              <template #default>{{ result.total_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>

      <!-- Decay -->
      <NCard title="Memory Decay" size="small">
        <NGrid :cols="4" :x-gap="12">
          <NGi><NStatistic label="Scanned" :value="result.decay.scanned" /></NGi>
          <NGi><NStatistic label="Updated" :value="result.decay.updated" /></NGi>
          <NGi><NStatistic label="Archived" :value="result.decay.archived" /></NGi>
          <NGi>
            <NStatistic label="Elapsed">
              <template #default>{{ result.decay.elapsed_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>

      <!-- Graph Evolution -->
      <NCard title="Graph Evolution" size="small">
        <NGrid :cols="4" :x-gap="12">
          <NGi><NStatistic label="Pairs Scanned" :value="result.evolution.pairs_scanned" /></NGi>
          <NGi><NStatistic label="Nodes Merged" :value="result.evolution.nodes_merged" /></NGi>
          <NGi><NStatistic label="Edges Strengthened" :value="result.evolution.edges_strengthened" /></NGi>
          <NGi>
            <NStatistic label="Elapsed">
              <template #default>{{ result.evolution.elapsed_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>

      <!-- Auto Promote -->
      <NCard title="Auto Promote" size="small">
        <NGrid :cols="3" :x-gap="12">
          <NGi><NStatistic label="Scanned" :value="result.promotion.scanned" /></NGi>
          <NGi><NStatistic label="Promoted" :value="result.promotion.promoted" /></NGi>
          <NGi>
            <NStatistic label="Elapsed">
              <template #default>{{ result.promotion.elapsed_ms }}<span style="font-size:12px;color:#999">ms</span></template>
            </NStatistic>
          </NGi>
        </NGrid>
      </NCard>
    </template>
  </NSpace>
</template>
