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
} from '@vicons/ionicons5'
import { NIcon } from 'naive-ui'

const route = useRoute()

function renderIcon(icon: any) {
  return () => h(NIcon, null, { default: () => h(icon) })
}

const menuOptions: MenuOption[] = [
  {
    label: () => h(RouterLink, { to: '/' }, { default: () => 'Overview' }),
    key: 'overview',
    icon: renderIcon(ServerOutline),
  },
  {
    label: () => h(RouterLink, { to: '/memory' }, { default: () => 'Memory Browser' }),
    key: 'memory',
    icon: renderIcon(LayersOutline),
  },
  {
    label: () => h(RouterLink, { to: '/graph' }, { default: () => 'Knowledge Graph' }),
    key: 'graph',
    icon: renderIcon(GitNetworkOutline),
  },
  {
    label: () => h(RouterLink, { to: '/audit' }, { default: () => 'Audit Trail' }),
    key: 'audit',
    icon: renderIcon(ListOutline),
  },
  {
    label: () => h(RouterLink, { to: '/benchmarks' }, { default: () => 'Benchmarks' }),
    key: 'benchmarks',
    icon: renderIcon(SpeedometerOutline),
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
