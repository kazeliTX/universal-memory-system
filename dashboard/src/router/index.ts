import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'overview',
      component: () => import('@/views/Overview.vue'),
    },
    {
      path: '/memory',
      name: 'memory',
      component: () => import('@/views/MemoryBrowser.vue'),
    },
    {
      path: '/graph',
      name: 'graph',
      component: () => import('@/views/GraphExplorer.vue'),
    },
    {
      path: '/audit',
      name: 'audit',
      component: () => import('@/views/AuditTrail.vue'),
    },
    {
      path: '/ingest',
      name: 'ingest',
      component: () => import('@/views/DocumentIngest.vue'),
    },
    {
      path: '/tags',
      name: 'tags',
      component: () => import('@/views/TagExplorer.vue'),
    },
    {
      path: '/benchmarks',
      name: 'benchmarks',
      component: () => import('@/views/Benchmarks.vue'),
    },
    {
      path: '/consolidation',
      name: 'consolidation',
      component: () => import('@/views/Consolidation.vue'),
    },
    {
      path: '/agents',
      name: 'agents',
      component: () => import('@/views/AgentManager.vue'),
    },
    {
      path: '/traces',
      name: 'traces',
      component: () => import('@/views/ModelTraces.vue'),
    },
  ],
})

export default router
