<script setup lang="ts">
import { h } from 'vue'
import { RouterLink, RouterView, useRoute } from 'vue-router'
import {
  NConfigProvider,
  NLayout,
  NLayoutSider,
  NMenu,
  NSpace,
  NText,
  darkTheme,
  type MenuOption,
} from 'naive-ui'
import {
  ServerOutline,
  LayersOutline,
  GitNetworkOutline,
  ListOutline,
  SpeedometerOutline,
  CloudUploadOutline,
  PricetagsOutline,
  SyncOutline,
  PeopleOutline,
  AnalyticsOutline,
  CreateOutline,
} from '@vicons/ionicons5'
import { NIcon } from 'naive-ui'

const route = useRoute()

function renderIcon(icon: any) {
  return () => h(NIcon, null, { default: () => h(icon) })
}

const menuOptions: MenuOption[] = [
  {
    label: () => h(RouterLink, { to: '/' }, { default: () => '系统概览' }),
    key: 'overview',
    icon: renderIcon(ServerOutline),
  },
  {
    label: () => h(RouterLink, { to: '/memory' }, { default: () => '记忆浏览' }),
    key: 'memory',
    icon: renderIcon(LayersOutline),
  },
  {
    label: () => h(RouterLink, { to: '/graph' }, { default: () => '图谱探索' }),
    key: 'graph',
    icon: renderIcon(GitNetworkOutline),
  },
  {
    label: () => h(RouterLink, { to: '/audit' }, { default: () => '审计日志' }),
    key: 'audit',
    icon: renderIcon(ListOutline),
  },
  {
    label: () => h(RouterLink, { to: '/ingest' }, { default: () => '文档摄入' }),
    key: 'ingest',
    icon: renderIcon(CloudUploadOutline),
  },
  {
    label: () => h(RouterLink, { to: '/tags' }, { default: () => '标签管理' }),
    key: 'tags',
    icon: renderIcon(PricetagsOutline),
  },
  {
    label: () => h(RouterLink, { to: '/benchmarks' }, { default: () => '性能基准' }),
    key: 'benchmarks',
    icon: renderIcon(SpeedometerOutline),
  },
  {
    label: () => h(RouterLink, { to: '/consolidation' }, { default: () => '记忆巩固' }),
    key: 'consolidation',
    icon: renderIcon(SyncOutline),
  },
  {
    label: () => h(RouterLink, { to: '/agents' }, { default: () => '智能体管理' }),
    key: 'agents',
    icon: renderIcon(PeopleOutline),
  },
  {
    label: () => h(RouterLink, { to: '/traces' }, { default: () => '模型追踪' }),
    key: 'traces',
    icon: renderIcon(AnalyticsOutline),
  },
  {
    label: () => h(RouterLink, { to: '/prompts' }, { default: () => 'Prompt编辑器' }),
    key: 'prompts',
    icon: renderIcon(CreateOutline),
  },
]

function getActiveKey(): string {
  return route.name?.toString() ?? 'overview'
}
</script>

<template>
  <NConfigProvider :theme="darkTheme">
    <NLayout has-sider style="height: 100vh">
      <NLayoutSider
        bordered
        :width="220"
        :collapsed-width="64"
        collapse-mode="width"
        show-trigger
        style="height: 100vh"
      >
        <NSpace vertical :size="0">
          <div style="padding: 16px 20px; font-weight: 700; font-size: 16px; color: #58a6ff">
            ⬡ UMMS
          </div>
          <NMenu
            :options="menuOptions"
            :value="getActiveKey()"
            :indent="24"
          />
        </NSpace>
      </NLayoutSider>

      <NLayout content-style="padding: 24px; overflow: auto;">
        <RouterView />
      </NLayout>
    </NLayout>
  </NConfigProvider>
</template>

<style>
body {
  margin: 0;
  padding: 0;
  font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace;
}
</style>
