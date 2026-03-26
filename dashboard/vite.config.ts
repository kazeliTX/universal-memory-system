import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    // Proxy API requests to embedded Axum server during development
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8720',
        changeOrigin: true,
      },
    },
  },
  // Tauri expects a fixed port for devUrl
  clearScreen: false,
})
