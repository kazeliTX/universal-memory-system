<script setup lang="ts">
defineProps<{ agentName?: string }>()
</script>

<template>
  <div class="thinking-container">
    <div class="thinking-avatar">
      <div class="thinking-dots">
        <span class="dot"></span>
        <span class="dot"></span>
        <span class="dot"></span>
      </div>
    </div>
    <span class="thinking-text">
      {{ agentName ? `${agentName} 正在思考...` : '思考中...' }}
    </span>
  </div>
</template>

<style scoped>
.thinking-container {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  animation: fadeIn 0.3s ease;
}

.thinking-avatar {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: rgba(255, 0, 110, 0.1);
  border: 1px solid rgba(255, 0, 110, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.thinking-dots {
  display: flex;
  gap: 4px;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #ff006e;
  animation: pulse 1.4s ease-in-out infinite;
}

.dot:nth-child(2) {
  animation-delay: 0.2s;
}

.dot:nth-child(3) {
  animation-delay: 0.4s;
}

.thinking-text {
  font-size: 13px;
  color: rgba(255, 0, 110, 0.7);
  font-family: 'JetBrains Mono', monospace;
}

@keyframes pulse {
  0%, 80%, 100% {
    opacity: 0.3;
    transform: scale(0.8);
  }
  40% {
    opacity: 1;
    transform: scale(1.2);
  }
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>
