import { defineConfig, loadEnv } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const proxyTarget = (env.VITE_API_BASE ?? 'http://localhost:8080').replace(/\/$/, '')
  const proxyConfig = {
    '/api': {
      target: proxyTarget,
      changeOrigin: true,
      ws: true,
    },
  }

  return {
    plugins: [svelte()],
    server: {
      proxy: proxyConfig,
    },
    preview: {
      proxy: proxyConfig,
    },
  }
})
