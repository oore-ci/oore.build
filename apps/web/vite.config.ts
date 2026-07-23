import { URL, fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'
import viteReact from '@vitejs/plugin-react'

import tailwindcss from '@tailwindcss/vite'

import { tanstackRouter } from '@tanstack/router-plugin/vite'

type ReleaseChannel = 'alpha' | 'beta' | 'stable' | 'dev'

function getReleaseChannel(): ReleaseChannel {
  const configuredChannel = process.env.OORE_WEB_RELEASE_CHANNEL
  if (
    configuredChannel === 'alpha' ||
    configuredChannel === 'beta' ||
    configuredChannel === 'stable'
  ) {
    return configuredChannel
  }

  const releaseTag = process.env.RELEASE_TAG
  if (releaseTag?.includes('-alpha.')) return 'alpha'
  if (releaseTag?.includes('-beta.')) return 'beta'
  if (releaseTag?.startsWith('v')) return 'stable'
  return 'dev'
}

// https://vitejs.dev/config/
export default defineConfig({
  define: {
    'import.meta.env.VITE_RELEASE_CHANNEL': JSON.stringify(getReleaseChannel()),
  },
  plugins: [
    devtools(),
    tanstackRouter({
      target: 'react',
      autoCodeSplitting: true,
      quoteStyle: 'single',
    }),
    viteReact(),
    tailwindcss(),
  ],
  build: {
    manifest: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) return

          // Framework/runtime chunks are stable across route deployments.
          if (
            id.includes('/node_modules/react-dom/') ||
            id.includes('/node_modules/react/') ||
            id.includes('/node_modules/scheduler/')
          )
            return 'react-vendor'

          if (
            id.includes('/@tanstack/react-router/') ||
            id.includes('/@tanstack/router-') ||
            id.includes('/@tanstack/history') ||
            id.includes('/tiny-invariant/') ||
            id.includes('/tiny-warning/')
          )
            return 'router-vendor'

          // Keep the TanStack routing and query runtimes in one stable chunk.
          if (
            id.includes('/@tanstack/react-query/') ||
            id.includes('/@tanstack/query-core/')
          )
            return 'router-vendor'

          // Toast notifications
          if (id.includes('/sonner/')) return 'sonner'
        },
      },
    },
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    proxy: {
      '/v1': {
        // When running under portless, proxy to the named API URL.
        // Falls back to direct daemon address for non-portless setups.
        target:
          process.env.OORED_URL ||
          (process.env.PORTLESS_URL
            ? 'http://api.oore.localhost:1355'
            : 'http://127.0.0.1:8787'),
        changeOrigin: true,
        ws: true,
      },
    },
  },
})
