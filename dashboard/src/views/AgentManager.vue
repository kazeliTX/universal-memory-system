<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  NCard, NSpace, NButton, NAlert, NTag, NModal, NForm, NFormItem,
  NInput, NDynamicTags, NDataTable, NStatistic, NGrid, NGi, NEmpty,
  NPopconfirm,
} from 'naive-ui'
import { listAgents, createAgent, updateAgent, deleteAgent, getAgent } from '@/api/client'
import type { AgentPersonaResponse, CreateAgentRequest } from '@/types'

const agents = ref<AgentPersonaResponse[]>([])
const loading = ref(false)
const error = ref<string | null>(null)

// Create modal
const showCreate = ref(false)
const createForm = ref<CreateAgentRequest>({
  agent_id: '',
  name: '',
  role: '',
  description: '',
  expertise: [],
})
const creating = ref(false)

// Detail panel
const selectedAgent = ref<AgentPersonaResponse | null>(null)

// Edit modal
const showEdit = ref(false)
const editForm = ref({
  name: '',
  role: '',
  description: '',
  expertise: [] as string[],
})
const saving = ref(false)

async function loadAgents() {
  loading.value = true
  error.value = null
  try {
    const res = await listAgents()
    agents.value = res.agents
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    loading.value = false
  }
}

async function handleCreate() {
  creating.value = true
  error.value = null
  try {
    await createAgent(createForm.value)
    showCreate.value = false
    createForm.value = { agent_id: '', name: '', role: '', description: '', expertise: [] }
    await loadAgents()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    creating.value = false
  }
}

function openEdit(agent: AgentPersonaResponse) {
  editForm.value = {
    name: agent.name,
    role: agent.role,
    description: agent.description,
    expertise: [...agent.expertise],
  }
  selectedAgent.value = agent
  showEdit.value = true
}

async function handleSave() {
  if (!selectedAgent.value) return
  saving.value = true
  error.value = null
  try {
    await updateAgent(selectedAgent.value.agent_id, {
      name: editForm.value.name,
      role: editForm.value.role,
      description: editForm.value.description,
      expertise: editForm.value.expertise,
    })
    showEdit.value = false
    await loadAgents()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    saving.value = false
  }
}

async function handleDelete(agentId: string) {
  error.value = null
  try {
    await deleteAgent(agentId)
    if (selectedAgent.value?.agent_id === agentId) {
      selectedAgent.value = null
    }
    await loadAgents()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

function selectAgent(agent: AgentPersonaResponse) {
  selectedAgent.value = agent
}

onMounted(loadAgents)

const columns = [
  {
    title: 'Name',
    key: 'name',
    width: 140,
  },
  {
    title: 'Role',
    key: 'role',
    width: 180,
  },
  {
    title: 'Expertise',
    key: 'expertise',
    render(row: AgentPersonaResponse) {
      return row.expertise.length + ' tags'
    },
    width: 100,
  },
  {
    title: 'Vectors',
    key: 'vector_count',
    width: 80,
  },
  {
    title: 'Cache',
    key: 'cache',
    width: 80,
    render(row: AgentPersonaResponse) {
      return `${row.cache_l0 + row.cache_l1}`
    },
  },
]
</script>

<template>
  <NSpace vertical :size="16">
    <NSpace align="center" justify="space-between">
      <h2 style="margin: 0; color: #e6edf3">Agent Manager (M7)</h2>
      <NSpace :size="8">
        <NButton size="small" @click="loadAgents" :loading="loading">Refresh</NButton>
        <NButton type="primary" size="small" @click="showCreate = true">Create Agent</NButton>
      </NSpace>
    </NSpace>

    <NAlert v-if="error" type="error" title="Error" closable @close="error = null">
      {{ error }}
    </NAlert>

    <!-- Agent table -->
    <NCard title="Agents" size="small">
      <NDataTable
        :columns="columns"
        :data="agents"
        :loading="loading"
        :row-key="(row: AgentPersonaResponse) => row.agent_id"
        :row-props="(row: AgentPersonaResponse) => ({
          style: 'cursor: pointer',
          onClick: () => selectAgent(row),
        })"
        size="small"
        :bordered="false"
      />
      <NEmpty v-if="!loading && agents.length === 0" description="No agents found" />
    </NCard>

    <!-- Detail panel -->
    <NCard v-if="selectedAgent" :title="selectedAgent.name" size="small">
      <template #header-extra>
        <NSpace :size="8">
          <NButton size="tiny" @click="openEdit(selectedAgent!)">Edit</NButton>
          <NPopconfirm @positive-click="handleDelete(selectedAgent!.agent_id)">
            <template #trigger>
              <NButton size="tiny" type="error">Delete</NButton>
            </template>
            Delete agent "{{ selectedAgent.agent_id }}"? This will not remove its memories.
          </NPopconfirm>
        </NSpace>
      </template>

      <NSpace vertical :size="12">
        <NSpace :size="8">
          <NTag type="primary" size="small">{{ selectedAgent.agent_id }}</NTag>
          <NTag type="info" size="small">{{ selectedAgent.role }}</NTag>
        </NSpace>

        <div style="color: #999; font-size: 13px">{{ selectedAgent.description }}</div>

        <div>
          <div style="color: #666; font-size: 12px; margin-bottom: 4px">Expertise</div>
          <NSpace :size="4">
            <NTag
              v-for="tag in selectedAgent.expertise"
              :key="tag"
              size="small"
              type="success"
            >
              {{ tag }}
            </NTag>
          </NSpace>
        </div>

        <NGrid :cols="4" :x-gap="12">
          <NGi><NStatistic label="L0 Cache" :value="selectedAgent.cache_l0" /></NGi>
          <NGi><NStatistic label="L1 Cache" :value="selectedAgent.cache_l1" /></NGi>
          <NGi><NStatistic label="Vectors" :value="selectedAgent.vector_count" /></NGi>
          <NGi>
            <div>
              <div style="color: #999; font-size: 12px; margin-bottom: 4px">Created</div>
              <div style="font-size: 13px">{{ selectedAgent.created_at ? new Date(selectedAgent.created_at).toLocaleDateString() : '-' }}</div>
            </div>
          </NGi>
        </NGrid>

        <!-- Retrieval config overrides -->
        <NCard title="Retrieval Config Overrides" size="small" embedded>
          <NGrid :cols="4" :x-gap="12">
            <NGi>
              <div style="color: #999; font-size: 12px">BM25 Weight</div>
              <div>{{ selectedAgent.retrieval_config.bm25_weight ?? 'default' }}</div>
            </NGi>
            <NGi>
              <div style="color: #999; font-size: 12px">Min Score</div>
              <div>{{ selectedAgent.retrieval_config.min_score ?? 'default' }}</div>
            </NGi>
            <NGi>
              <div style="color: #999; font-size: 12px">Top K Final</div>
              <div>{{ selectedAgent.retrieval_config.top_k_final ?? 'default' }}</div>
            </NGi>
            <NGi>
              <div style="color: #999; font-size: 12px">LIF Hops</div>
              <div>{{ selectedAgent.retrieval_config.lif_hops ?? 'default' }}</div>
            </NGi>
          </NGrid>
        </NCard>
      </NSpace>
    </NCard>

    <!-- Create Modal -->
    <NModal
      v-model:show="showCreate"
      title="Create Agent"
      preset="card"
      style="width: 500px"
      :mask-closable="false"
    >
      <NForm :model="createForm" label-placement="top">
        <NFormItem label="Agent ID" required>
          <NInput v-model:value="createForm.agent_id" placeholder="e.g. my-agent" />
        </NFormItem>
        <NFormItem label="Name" required>
          <NInput v-model:value="createForm.name" placeholder="Display name" />
        </NFormItem>
        <NFormItem label="Role">
          <NInput v-model:value="createForm.role" placeholder="e.g. Software Engineer" />
        </NFormItem>
        <NFormItem label="Description">
          <NInput
            v-model:value="createForm.description"
            type="textarea"
            placeholder="What this agent is for"
            :rows="2"
          />
        </NFormItem>
        <NFormItem label="Expertise Tags">
          <NDynamicTags v-model:value="createForm.expertise" />
        </NFormItem>
      </NForm>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showCreate = false">Cancel</NButton>
          <NButton
            type="primary"
            :loading="creating"
            :disabled="!createForm.agent_id || !createForm.name"
            @click="handleCreate"
          >
            Create
          </NButton>
        </NSpace>
      </template>
    </NModal>

    <!-- Edit Modal -->
    <NModal
      v-model:show="showEdit"
      title="Edit Agent"
      preset="card"
      style="width: 500px"
      :mask-closable="false"
    >
      <NForm :model="editForm" label-placement="top">
        <NFormItem label="Name">
          <NInput v-model:value="editForm.name" />
        </NFormItem>
        <NFormItem label="Role">
          <NInput v-model:value="editForm.role" />
        </NFormItem>
        <NFormItem label="Description">
          <NInput v-model:value="editForm.description" type="textarea" :rows="2" />
        </NFormItem>
        <NFormItem label="Expertise Tags">
          <NDynamicTags v-model:value="editForm.expertise" />
        </NFormItem>
      </NForm>
      <template #footer>
        <NSpace justify="end">
          <NButton @click="showEdit = false">Cancel</NButton>
          <NButton type="primary" :loading="saving" @click="handleSave">Save</NButton>
        </NSpace>
      </template>
    </NModal>
  </NSpace>
</template>
