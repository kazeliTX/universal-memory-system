<script setup lang="ts">
import { ref } from 'vue'
import { NConfigProvider, darkTheme, NLayout, NLayoutSider, NLayoutContent } from 'naive-ui'
import AgentSelector from './components/AgentSelector.vue'
import ChatWindow from './components/ChatWindow.vue'

const selectedAgent = ref('coder')
</script>

<template>
  <NConfigProvider :theme="darkTheme">
    <NLayout has-sider style="height: 100vh">
      <NLayoutSider width="240" bordered style="background: #0d1117">
        <AgentSelector v-model:agent="selectedAgent" />
      </NLayoutSider>
      <NLayoutContent style="background: #161b22">
        <ChatWindow :agent-id="selectedAgent" />
      </NLayoutContent>
    </NLayout>
  </NConfigProvider>
</template>
