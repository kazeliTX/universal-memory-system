/**
 * Shared test utilities for Chat frontend tests.
 */
import { createApp, type App } from 'vue'

/**
 * Helper to test Vue composables that use lifecycle hooks (onMounted/onUnmounted).
 * Returns the composable's return value and the app instance for cleanup.
 */
export function withSetup<T>(composable: () => T): [T, App] {
  let result!: T
  const app = createApp({
    setup() {
      result = composable()
      return () => {} // render nothing
    },
  })
  app.mount(document.createElement('div'))
  return [result, app]
}

/**
 * Creates a mock agent for testing.
 */
export function mockAgent(overrides: Record<string, unknown> = {}) {
  return {
    agent_id: 'coder',
    name: 'Coder',
    role: '开发工程师',
    description: '编程助手',
    expertise: ['rust', 'typescript'],
    ...overrides,
  }
}

/**
 * Creates a mock chat session for testing.
 */
export function mockSession(overrides: Record<string, unknown> = {}) {
  return {
    id: 'session-1',
    agentId: 'coder',
    title: '测试对话',
    messages: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
    ...overrides,
  }
}
