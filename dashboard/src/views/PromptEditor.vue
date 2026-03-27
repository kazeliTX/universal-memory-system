<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from 'vue'
import {
  NCard, NSpace, NButton, NAlert, NTag, NInput, NRadioGroup, NRadioButton,
  NSwitch, NCollapse, NCollapseItem, NSelect, NModal, NEmpty, NBadge,
  NScrollbar, NTooltip, NPopconfirm, NSpin, NDivider, NText,
} from 'naive-ui'
import {
  listAgents, getPromptConfig, savePromptConfig, switchPromptMode,
  addBlock, updateBlock, deleteBlock, reorderBlocks,
  addVariant, selectVariant,
  listWarehouses, listVariables, previewPrompt,
} from '@/api/client'
import type {
  AgentPersonaResponse, AgentPromptConfig, PromptBlock,
  PromptMode, PromptWarehouse, PromptVariable,
} from '@/types'

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const agents = ref<AgentPersonaResponse[]>([])
const selectedAgentId = ref<string | null>(null)
const config = ref<AgentPromptConfig | null>(null)
const warehouses = ref<PromptWarehouse[]>([])
const variables = ref<PromptVariable[]>([])
const preview = ref('')
const loading = ref(false)
const saving = ref(false)
const error = ref<string | null>(null)

// UI state
const showPreview = ref(true)
const showImportModal = ref(false)
const importSourceWarehouse = ref<string | null>(null)

// Preset mode
const presetFiles = ref<string[]>([
  'default-coder.md',
  'default-researcher.md',
  'default-writer.md',
  'creative-mode.md',
  'strict-mode.md',
])

// Block types
const BLOCK_TYPES = [
  { label: 'System', value: 'System', color: '#3b82f6' },
  { label: 'Memory', value: 'Memory', color: '#22c55e' },
  { label: 'Diary', value: 'Diary', color: '#a855f7' },
  { label: 'Rules', value: 'Rules', color: '#f59e0b' },
  { label: 'Reasoning', value: 'Reasoning', color: '#06b6d4' },
  { label: 'Safety', value: 'Safety', color: '#ef4444' },
  { label: 'Format', value: 'Format', color: '#8b5cf6' },
  { label: 'Task', value: 'Task', color: '#ec4899' },
  { label: 'Custom', value: 'Custom', color: '#6b7280' },
]

function blockTypeColor(type: string): string {
  return BLOCK_TYPES.find(t => t.value === type)?.color ?? '#6b7280'
}

const blockTypeOptions = BLOCK_TYPES.map(t => ({ label: t.label, value: t.value }))

// Computed
const selectedAgent = computed(() =>
  agents.value.find(a => a.agent_id === selectedAgentId.value) ?? null
)

const currentMode = computed<PromptMode>({
  get: () => config.value?.mode ?? 'modular',
  set: (v) => { if (config.value) config.value.mode = v },
})

const sortedBlocks = computed(() =>
  config.value
    ? [...config.value.blocks].sort((a, b) => a.order - b.order)
    : []
)

const charCount = computed(() => config.value?.original_prompt?.length ?? 0)

const tokenEstimate = computed(() => {
  const text = preview.value
  // rough estimate: ~1.5 chars per token for Chinese
  return Math.ceil(text.length / 1.5)
})

const resolvedPreview = computed(() => {
  let text = preview.value
  const now = new Date()
  const replacements: Record<string, string> = {
    '{{AgentName}}': selectedAgent.value?.name ?? 'Agent',
    '{{AgentRole}}': selectedAgent.value?.role ?? 'Assistant',
    '{{DateTime}}': now.toLocaleString('zh-CN'),
    '{{Date}}': now.toLocaleDateString('zh-CN'),
    '{{UserLanguage}}': '中文',
    '{{MemoryContext}}': '[记忆上下文将在运行时注入]',
    '{{RecentHistory}}': '[最近对话历史将在运行时注入]',
  }
  for (const [key, val] of Object.entries(replacements)) {
    text = text.replaceAll(key, val)
  }
  return text
})

function formatVar(name: string): string {
  return '{{' + name + '}}'
}

const importWarehouseBlocks = computed(() => {
  if (!importSourceWarehouse.value) return []
  const wh = warehouses.value.find(w => w.name === importSourceWarehouse.value)
  return wh?.blocks ?? []
})

// ---------------------------------------------------------------------------
// Data loading
// ---------------------------------------------------------------------------

async function loadAgents() {
  loading.value = true
  error.value = null
  try {
    const res = await listAgents()
    agents.value = res.agents
    const first = agents.value[0]
    if (first && !selectedAgentId.value) {
      selectedAgentId.value = first.agent_id
    }
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    loading.value = false
  }
}

async function loadConfig() {
  if (!selectedAgentId.value) return
  loading.value = true
  error.value = null
  try {
    config.value = await getPromptConfig(selectedAgentId.value)
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    loading.value = false
  }
}

async function loadWarehouses() {
  try {
    warehouses.value = await listWarehouses()
  } catch (e: any) {
    console.warn('Failed to load warehouses:', e)
  }
}

async function loadVariables() {
  try {
    variables.value = await listVariables()
  } catch (e: any) {
    console.warn('Failed to load variables:', e)
  }
}

async function refreshPreview() {
  if (!selectedAgentId.value) return
  try {
    preview.value = await previewPrompt(selectedAgentId.value)
  } catch (_) {
    // ignore preview errors
  }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

async function handleSave() {
  if (!selectedAgentId.value || !config.value) return
  saving.value = true
  error.value = null
  try {
    await savePromptConfig(selectedAgentId.value, config.value)
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  } finally {
    saving.value = false
  }
}

async function handleModeSwitch(mode: PromptMode) {
  if (!selectedAgentId.value || !config.value) return
  await handleSave()
  config.value.mode = mode
  try {
    await switchPromptMode(selectedAgentId.value, mode)
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

// Block operations
const newBlockType = ref('Custom')

function generateBlockId(): string {
  return 'blk-' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6)
}

async function handleAddBlock() {
  if (!selectedAgentId.value || !config.value) return
  const block: PromptBlock = {
    id: generateBlockId(),
    name: '新模块',
    block_type: newBlockType.value,
    content: '',
    variants: [''],
    selected_variant: 0,
    enabled: true,
    order: config.value.blocks.length,
  }
  config.value.blocks.push(block)
  try {
    await addBlock(selectedAgentId.value, block)
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

async function handleDeleteBlock(blockId: string) {
  if (!selectedAgentId.value || !config.value) return
  config.value.blocks = config.value.blocks.filter(b => b.id !== blockId)
  // reorder
  config.value.blocks.forEach((b, i) => b.order = i)
  try {
    await deleteBlock(selectedAgentId.value, blockId)
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

async function handleMoveBlock(blockId: string, direction: 'up' | 'down') {
  if (!config.value) return
  const blocks = [...config.value.blocks].sort((a, b) => a.order - b.order)
  const idx = blocks.findIndex(b => b.id === blockId)
  if (idx < 0) return
  const targetIdx = direction === 'up' ? idx - 1 : idx + 1
  if (targetIdx < 0 || targetIdx >= blocks.length) return
  // swap order
  const current = blocks[idx]!
  const target = blocks[targetIdx]!
  const tmp = current.order
  current.order = target.order
  target.order = tmp
  config.value.blocks = blocks
  try {
    await reorderBlocks(selectedAgentId.value!, blocks.sort((a, b) => a.order - b.order).map(b => b.id))
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

async function handleToggleBlock(block: PromptBlock) {
  if (!selectedAgentId.value) return
  block.enabled = !block.enabled
  try {
    await updateBlock(selectedAgentId.value, block.id, { enabled: block.enabled })
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

async function handleBlockContentChange(block: PromptBlock) {
  if (!selectedAgentId.value) return
  // Update the current variant too
  if (block.variants.length > block.selected_variant) {
    block.variants[block.selected_variant] = block.content
  }
  try {
    await updateBlock(selectedAgentId.value, block.id, {
      content: block.content,
      variants: block.variants,
    })
  } catch (_) { /* debounce errors */ }
}

async function handleBlockNameChange(block: PromptBlock) {
  if (!selectedAgentId.value) return
  try {
    await updateBlock(selectedAgentId.value, block.id, { name: block.name })
  } catch (_) { /* ignore */ }
}

// Variant operations
async function handleSelectVariant(block: PromptBlock, index: number) {
  if (!selectedAgentId.value) return
  block.selected_variant = index
  block.content = block.variants[index] ?? block.content
  try {
    await selectVariant(selectedAgentId.value, block.id, index)
    await refreshPreview()
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

async function handleAddVariant(block: PromptBlock) {
  if (!selectedAgentId.value) return
  const newContent = block.content
  block.variants.push(newContent)
  const newIdx = block.variants.length - 1
  block.selected_variant = newIdx
  try {
    await addVariant(selectedAgentId.value, block.id, newContent)
    await selectVariant(selectedAgentId.value, block.id, newIdx)
  } catch (e: any) {
    error.value = e.message ?? String(e)
  }
}

function handleDeleteVariant(block: PromptBlock, index: number) {
  if (block.variants.length <= 1) return
  block.variants.splice(index, 1)
  if (block.selected_variant >= block.variants.length) {
    block.selected_variant = block.variants.length - 1
  }
  block.content = block.variants[block.selected_variant] ?? block.content
}

// Import from warehouse
function handleImportBlock(whBlock: PromptBlock) {
  if (!config.value) return
  const imported: PromptBlock = {
    ...JSON.parse(JSON.stringify(whBlock)),
    id: generateBlockId(),
    order: config.value.blocks.length,
  }
  config.value.blocks.push(imported)
  showImportModal.value = false
}

// Save block to warehouse
function handleSaveToWarehouse(block: PromptBlock) {
  const privateWh = warehouses.value.find(w => !w.is_global)
  if (!privateWh) return
  const copy: PromptBlock = {
    ...JSON.parse(JSON.stringify(block)),
    id: 'wh-' + Date.now().toString(36),
  }
  privateWh.blocks.push(copy)
}

// Preset
async function handlePresetSelect(path: string) {
  if (!config.value) return
  config.value.preset_path = path
  config.value.preset_content = `# 预设: ${path}\n\n这是预设文件 "${path}" 的内容。\n\n实际内容将从文件系统中加载。`
  await refreshPreview()
}

// Watchers
watch(selectedAgentId, () => {
  loadConfig()
})

onMounted(async () => {
  await Promise.all([loadAgents(), loadWarehouses(), loadVariables()])
})
</script>

<template>
  <div class="prompt-editor">
    <!-- Header -->
    <NSpace align="center" justify="space-between" style="margin-bottom: 16px">
      <h2 style="margin: 0; color: #e6edf3">Prompt 编辑器</h2>
      <NSpace :size="8">
        <NButton size="small" @click="refreshPreview">刷新预览</NButton>
        <NButton type="primary" size="small" :loading="saving" @click="handleSave">
          保存配置
        </NButton>
      </NSpace>
    </NSpace>

    <NAlert v-if="error" type="error" title="错误" closable @close="error = null" style="margin-bottom: 12px">
      {{ error }}
    </NAlert>

    <div class="editor-layout">
      <!-- Left Panel: Agent list + Warehouse -->
      <div class="left-panel">
        <NScrollbar style="max-height: calc(100vh - 140px)">
          <!-- Agent List -->
          <div class="panel-section">
            <div class="panel-section-title">智能体列表</div>
            <div v-if="loading && agents.length === 0" style="padding: 12px; text-align: center">
              <NSpin size="small" />
            </div>
            <div
              v-for="agent in agents"
              :key="agent.agent_id"
              class="agent-item"
              :class="{ active: selectedAgentId === agent.agent_id }"
              @click="selectedAgentId = agent.agent_id"
            >
              <div class="agent-item-name">{{ agent.name }}</div>
              <div class="agent-item-id">{{ agent.agent_id }}</div>
              <NTag
                v-if="selectedAgentId === agent.agent_id && config"
                size="tiny"
                :bordered="false"
                :type="config.mode === 'modular' ? 'success' : config.mode === 'original' ? 'info' : 'warning'"
                style="margin-top: 4px"
              >
                {{ config.mode === 'modular' ? '积木' : config.mode === 'original' ? '原始' : '预设' }}
              </NTag>
            </div>
            <NEmpty v-if="!loading && agents.length === 0" description="暂无智能体" size="small" />
          </div>

          <NDivider style="margin: 8px 0" />

          <!-- Warehouse -->
          <div class="panel-section">
            <div class="panel-section-title">模块仓库</div>
            <NCollapse :default-expanded-names="['全局仓库']" arrow-placement="left">
              <NCollapseItem
                v-for="wh in warehouses"
                :key="wh.name"
                :title="wh.name"
                :name="wh.name"
              >
                <div
                  v-for="block in wh.blocks"
                  :key="block.id"
                  class="warehouse-block"
                >
                  <div style="display: flex; align-items: center; gap: 6px; margin-bottom: 4px">
                    <span
                      class="type-badge"
                      :style="{ backgroundColor: blockTypeColor(block.block_type) + '22', color: blockTypeColor(block.block_type), borderColor: blockTypeColor(block.block_type) + '44' }"
                    >
                      {{ block.block_type }}
                    </span>
                    <span class="warehouse-block-name">{{ block.name }}</span>
                  </div>
                  <div class="warehouse-block-preview">
                    {{ block.content.slice(0, 50) }}{{ block.content.length > 50 ? '...' : '' }}
                  </div>
                  <NButton
                    size="tiny"
                    quaternary
                    type="primary"
                    style="margin-top: 4px"
                    @click="handleImportBlock(block)"
                  >
                    导入
                  </NButton>
                </div>
                <NEmpty v-if="wh.blocks.length === 0" description="空仓库" size="small" />
              </NCollapseItem>
            </NCollapse>
          </div>
        </NScrollbar>
      </div>

      <!-- Main Editor Area -->
      <div class="main-panel">
        <NSpin :show="loading && !!selectedAgentId">
          <template v-if="config">
            <!-- Mode Switcher -->
            <div class="mode-switcher">
              <span class="mode-label">模式:</span>
              <NRadioGroup
                :value="currentMode"
                size="small"
                @update:value="handleModeSwitch"
              >
                <NRadioButton value="original">原始模式</NRadioButton>
                <NRadioButton value="modular">积木模式</NRadioButton>
                <NRadioButton value="preset">预设模式</NRadioButton>
              </NRadioGroup>
              <div style="flex: 1" />
              <NText depth="3" style="font-size: 12px">
                上次更新: {{ new Date(config.updated_at).toLocaleString('zh-CN') }}
              </NText>
            </div>

            <!-- ========== ORIGINAL MODE ========== -->
            <div v-if="currentMode === 'original'" class="mode-content">
              <NInput
                v-model:value="config.original_prompt"
                type="textarea"
                placeholder="在此编辑原始 Prompt..."
                :autosize="{ minRows: 12, maxRows: 30 }"
                style="font-family: 'Cascadia Code', monospace; font-size: 13px"
              />
              <div class="original-footer">
                <div class="variable-hints">
                  <span class="hint-label">可用变量:</span>
                  <NTag
                    v-for="v in variables"
                    :key="v.name"
                    size="tiny"
                    :bordered="false"
                    style="cursor: help"
                    :title="v.description"
                  >
                    {{ formatVar(v.name) }}
                  </NTag>
                </div>
                <NText depth="3" style="font-size: 12px">
                  字符数: {{ charCount }}
                </NText>
              </div>
            </div>

            <!-- ========== MODULAR MODE ========== -->
            <div v-else-if="currentMode === 'modular'" class="mode-content">
              <NScrollbar style="max-height: calc(100vh - 380px)">
                <div class="blocks-container">
                  <TransitionGroup name="block-list">
                    <div
                      v-for="(block, idx) in sortedBlocks"
                      :key="block.id"
                      class="block-card"
                      :class="{ disabled: !block.enabled }"
                    >
                      <!-- Block Header -->
                      <div class="block-header">
                        <div class="block-header-left">
                          <span
                            class="type-badge"
                            :style="{ backgroundColor: blockTypeColor(block.block_type) + '22', color: blockTypeColor(block.block_type), borderColor: blockTypeColor(block.block_type) + '44' }"
                          >
                            {{ block.block_type }}
                          </span>
                          <NInput
                            :value="block.name"
                            size="small"
                            :bordered="false"
                            style="max-width: 200px; font-weight: 600"
                            @update:value="(v: string) => { block.name = v }"
                            @blur="handleBlockNameChange(block)"
                          />
                        </div>
                        <div class="block-header-right">
                          <NTooltip trigger="hover">
                            <template #trigger>
                              <NSwitch
                                :value="block.enabled"
                                size="small"
                                @update:value="() => handleToggleBlock(block)"
                              />
                            </template>
                            {{ block.enabled ? '已启用' : '已禁用' }}
                          </NTooltip>
                          <NButton
                            size="tiny"
                            quaternary
                            :disabled="idx === 0"
                            @click="handleMoveBlock(block.id, 'up')"
                          >
                            ▲
                          </NButton>
                          <NButton
                            size="tiny"
                            quaternary
                            :disabled="idx === sortedBlocks.length - 1"
                            @click="handleMoveBlock(block.id, 'down')"
                          >
                            ▼
                          </NButton>
                          <NTooltip trigger="hover">
                            <template #trigger>
                              <NButton
                                size="tiny"
                                quaternary
                                type="primary"
                                @click="handleSaveToWarehouse(block)"
                              >
                                仓
                              </NButton>
                            </template>
                            保存到私有仓库
                          </NTooltip>
                          <NPopconfirm @positive-click="handleDeleteBlock(block.id)">
                            <template #trigger>
                              <NButton size="tiny" quaternary type="error">✕</NButton>
                            </template>
                            确定删除模块 "{{ block.name }}"？
                          </NPopconfirm>
                        </div>
                      </div>

                      <!-- Block Body -->
                      <div class="block-body">
                        <NInput
                          v-model:value="block.content"
                          type="textarea"
                          :autosize="{ minRows: 3, maxRows: 12 }"
                          :disabled="!block.enabled"
                          style="font-family: 'Cascadia Code', monospace; font-size: 12px"
                          @update:value="() => handleBlockContentChange(block)"
                        />
                      </div>

                      <!-- Block Footer: Variants -->
                      <div class="block-footer">
                        <div class="variant-pills">
                          <span
                            v-for="(_, vIdx) in block.variants"
                            :key="vIdx"
                            class="variant-pill"
                            :class="{ active: block.selected_variant === vIdx }"
                            @click="handleSelectVariant(block, vIdx)"
                            @contextmenu.prevent="handleDeleteVariant(block, vIdx)"
                          >
                            变体{{ vIdx + 1 }}
                          </span>
                          <span
                            class="variant-pill add-variant"
                            @click="handleAddVariant(block)"
                          >
                            + 变体
                          </span>
                        </div>
                      </div>
                    </div>
                  </TransitionGroup>
                </div>
              </NScrollbar>

              <!-- Add block controls -->
              <div class="add-block-bar">
                <NSelect
                  v-model:value="newBlockType"
                  :options="blockTypeOptions"
                  size="small"
                  style="width: 130px"
                  placeholder="类型"
                />
                <NButton size="small" type="primary" @click="handleAddBlock">
                  + 添加 Block
                </NButton>
                <NButton size="small" @click="showImportModal = true">
                  从仓库导入
                </NButton>
              </div>
            </div>

            <!-- ========== PRESET MODE ========== -->
            <div v-else-if="currentMode === 'preset'" class="mode-content">
              <div class="preset-controls">
                <NSelect
                  :value="config.preset_path"
                  :options="presetFiles.map(f => ({ label: f, value: f }))"
                  placeholder="选择预设文件..."
                  size="small"
                  style="flex: 1"
                  @update:value="handlePresetSelect"
                />
                <NButton size="small" @click="presetFiles = [...presetFiles]">
                  刷新
                </NButton>
              </div>
              <div v-if="config.preset_content" class="preset-preview">
                <pre>{{ config.preset_content }}</pre>
              </div>
              <NEmpty v-else description="请选择一个预设文件" style="margin-top: 40px" />
            </div>

            <!-- ========== LIVE PREVIEW ========== -->
            <NDivider style="margin: 12px 0 8px 0" />
            <div class="preview-section">
              <div
                class="preview-header"
                @click="showPreview = !showPreview"
              >
                <span class="preview-toggle">{{ showPreview ? '▼' : '▶' }}</span>
                <span class="preview-title">实时预览</span>
                <NTag size="tiny" :bordered="false" type="info" style="margin-left: 8px">
                  ~{{ tokenEstimate }} tokens
                </NTag>
              </div>
              <div v-if="showPreview" class="preview-body">
                <NScrollbar style="max-height: 300px">
                  <pre class="preview-content">{{ resolvedPreview }}</pre>
                </NScrollbar>
              </div>
            </div>
          </template>

          <NEmpty v-else-if="!loading" description="请在左侧选择一个智能体" style="margin-top: 80px" />
        </NSpin>
      </div>
    </div>

    <!-- Import Modal -->
    <NModal
      v-model:show="showImportModal"
      title="从仓库导入模块"
      preset="card"
      style="width: 560px"
      :mask-closable="true"
    >
      <NSpace vertical :size="12">
        <NSelect
          v-model:value="importSourceWarehouse"
          :options="warehouses.map(w => ({ label: w.name + (w.is_global ? ' (全局)' : ''), value: w.name }))"
          placeholder="选择仓库..."
          size="small"
        />
        <div v-if="importWarehouseBlocks.length > 0">
          <div
            v-for="block in importWarehouseBlocks"
            :key="block.id"
            class="import-block-item"
          >
            <div style="display: flex; align-items: center; gap: 8px; flex: 1">
              <span
                class="type-badge"
                :style="{ backgroundColor: blockTypeColor(block.block_type) + '22', color: blockTypeColor(block.block_type), borderColor: blockTypeColor(block.block_type) + '44' }"
              >
                {{ block.block_type }}
              </span>
              <span style="font-weight: 500; color: #e6edf3">{{ block.name }}</span>
            </div>
            <NButton size="tiny" type="primary" @click="handleImportBlock(block)">
              导入
            </NButton>
          </div>
        </div>
        <NEmpty v-else-if="importSourceWarehouse" description="该仓库为空" size="small" />
        <NEmpty v-else description="请选择一个仓库" size="small" />
      </NSpace>
    </NModal>
  </div>
</template>

<style scoped>
.prompt-editor {
  color: #e6edf3;
  min-height: calc(100vh - 48px);
}

.editor-layout {
  display: flex;
  gap: 16px;
  min-height: calc(100vh - 140px);
}

/* ---- Left Panel ---- */
.left-panel {
  width: 220px;
  min-width: 220px;
  background: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  overflow: hidden;
}

.panel-section {
  padding: 8px;
}

.panel-section-title {
  font-size: 12px;
  font-weight: 600;
  color: #8b949e;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding: 4px 8px 8px;
}

.agent-item {
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
  margin-bottom: 2px;
}

.agent-item:hover {
  background: #1c2333;
}

.agent-item.active {
  background: #1f6feb22;
  border-left: 3px solid #58a6ff;
  padding-left: 7px;
}

.agent-item-name {
  font-size: 13px;
  font-weight: 600;
  color: #e6edf3;
}

.agent-item-id {
  font-size: 11px;
  color: #8b949e;
  font-family: 'Cascadia Code', monospace;
}

/* Warehouse */
.warehouse-block {
  padding: 8px;
  margin-bottom: 6px;
  background: #0d1117;
  border: 1px solid #21262d;
  border-radius: 6px;
}

.warehouse-block-name {
  font-size: 12px;
  color: #e6edf3;
  font-weight: 500;
}

.warehouse-block-preview {
  font-size: 11px;
  color: #8b949e;
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---- Main Panel ---- */
.main-panel {
  flex: 1;
  min-width: 0;
}

.mode-switcher {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  background: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  margin-bottom: 12px;
}

.mode-label {
  font-size: 13px;
  color: #8b949e;
  font-weight: 500;
}

/* Original mode */
.mode-content {
  min-height: 200px;
}

.original-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 8px;
  flex-wrap: wrap;
  gap: 8px;
}

.variable-hints {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}

.hint-label {
  font-size: 12px;
  color: #8b949e;
  margin-right: 4px;
}

/* Block cards */
.blocks-container {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 2px;
}

.block-card {
  background: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  overflow: hidden;
  transition: opacity 0.2s, border-color 0.2s;
}

.block-card:hover {
  border-color: #484f58;
}

.block-card.disabled {
  opacity: 0.4;
  background: repeating-linear-gradient(
    -45deg,
    #161b22,
    #161b22 8px,
    #1c2128 8px,
    #1c2128 16px
  );
}

.block-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid #21262d;
  gap: 8px;
}

.block-header-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
}

.block-header-right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.type-badge {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  border: 1px solid;
  white-space: nowrap;
}

.block-body {
  padding: 8px 12px;
}

.block-footer {
  padding: 6px 12px;
  border-top: 1px solid #21262d;
}

.variant-pills {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.variant-pill {
  display: inline-block;
  padding: 2px 10px;
  border-radius: 12px;
  font-size: 11px;
  cursor: pointer;
  background: #21262d;
  color: #8b949e;
  transition: all 0.15s;
  user-select: none;
}

.variant-pill:hover {
  background: #30363d;
  color: #e6edf3;
}

.variant-pill.active {
  background: #238636;
  color: #fff;
}

.variant-pill.add-variant {
  background: transparent;
  border: 1px dashed #30363d;
  color: #58a6ff;
}

.variant-pill.add-variant:hover {
  border-color: #58a6ff;
  background: #1f6feb11;
}

/* Add block bar */
.add-block-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
  padding: 10px 14px;
  background: #161b22;
  border: 1px dashed #30363d;
  border-radius: 8px;
}

/* Preset mode */
.preset-controls {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}

.preset-preview {
  background: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  padding: 16px;
}

.preset-preview pre {
  margin: 0;
  white-space: pre-wrap;
  font-family: 'Cascadia Code', monospace;
  font-size: 13px;
  color: #c9d1d9;
  line-height: 1.6;
}

/* Preview */
.preview-section {
  background: #161b22;
  border: 1px solid #30363d;
  border-radius: 8px;
  overflow: hidden;
}

.preview-header {
  display: flex;
  align-items: center;
  padding: 8px 14px;
  cursor: pointer;
  user-select: none;
}

.preview-header:hover {
  background: #1c2128;
}

.preview-toggle {
  font-size: 10px;
  color: #8b949e;
  margin-right: 8px;
  width: 12px;
}

.preview-title {
  font-size: 13px;
  font-weight: 600;
  color: #e6edf3;
}

.preview-body {
  border-top: 1px solid #21262d;
  padding: 12px 16px;
}

.preview-content {
  margin: 0;
  white-space: pre-wrap;
  font-family: 'Cascadia Code', monospace;
  font-size: 12px;
  color: #7ee787;
  line-height: 1.6;
}

/* Import modal */
.import-block-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: #0d1117;
  border: 1px solid #21262d;
  border-radius: 6px;
  margin-bottom: 6px;
}

/* Block list transition */
.block-list-enter-active,
.block-list-leave-active {
  transition: all 0.3s ease;
}

.block-list-enter-from,
.block-list-leave-to {
  opacity: 0;
  transform: translateY(-10px);
}

.block-list-move {
  transition: transform 0.3s ease;
}
</style>
