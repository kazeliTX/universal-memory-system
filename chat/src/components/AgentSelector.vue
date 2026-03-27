<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NCard, NSpace, NTag, NText, NSpin } from 'naive-ui'
import { listAgents } from '../api'
import type { AgentInfo } from '../types'

const props = defineProps<{ agent: string }>()
const emit = defineEmits<{ 'update:agent': [value: string] }>()

const agents = ref<AgentInfo[]>([])
const loading = ref(true)

onMounted(async () => {
  try {
    const data = await listAgents()
    agents.value = data.agents
  } catch (_e) {
    // Fallback with default agent
    agents.value = [
      { agent_id: 'coder', name: 'Coder', role: '开发工程师', description: '编程助手', expertise: [] },
    ]
  } finally {
    loading.value = false
  }
})

function selectAgent(id: string) {
  emit('update:agent', id)
}

function avatarChar(name: string): string {
  return name.charAt(0).toUpperCase()
}
</script>

<template>
  <div style="padding: 16px">
    <NText strong style="font-size: 18px; color: #58a6ff; display: block; margin-bottom: 16px">
      UMMS Chat
    </NText>
    <NText depth="3" style="display: block; margin-bottom: 12px; font-size: 12px">
      选择智能体
    </NText>
    <NSpin :show="loading">
      <NSpace vertical :size="8">
        <NCard
          v-for="a in agents"
          :key="a.agent_id"
          size="small"
          hoverable
          :style="{
            cursor: 'pointer',
            border: a.agent_id === props.agent ? '1px solid #58a6ff' : '1px solid #30363d',
            background: a.agent_id === props.agent ? '#1c2333' : '#0d1117',
          }"
          @click="selectAgent(a.agent_id)"
        >
          <div style="display: flex; align-items: center; gap: 10px">
            <div
              :style="{
                width: '36px',
                height: '36px',
                borderRadius: '50%',
                background: a.agent_id === props.agent ? '#58a6ff' : '#30363d',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontWeight: 'bold',
                fontSize: '16px',
                color: '#fff',
                flexShrink: 0,
              }"
            >
              {{ avatarChar(a.name) }}
            </div>
            <div style="min-width: 0">
              <NText strong style="display: block; color: #c9d1d9">{{ a.name }}</NText>
              <NText depth="3" style="font-size: 12px">{{ a.role }}</NText>
            </div>
          </div>
          <div v-if="a.expertise && a.expertise.length > 0" style="margin-top: 8px">
            <NTag
              v-for="tag in a.expertise.slice(0, 3)"
              :key="tag"
              size="tiny"
              :bordered="false"
              style="margin-right: 4px"
            >
              {{ tag }}
            </NTag>
          </div>
        </NCard>
      </NSpace>
    </NSpin>
  </div>
</template>
