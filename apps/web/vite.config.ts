import { URL, fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'
import react from '@vitejs/plugin-react'

import tailwindcss from '@tailwindcss/vite'

import { tanstackRouter } from '@tanstack/router-plugin/vite'

type ReleaseChannel = 'alpha' | 'beta' | 'stable' | 'dev'

function getReleaseChannel(): ReleaseChannel {
  const configuredChannel = process.env.OORE_WEB_RELEASE_CHANNEL
  const releaseTag = process.env.RELEASE_TAG

  switch (configuredChannel) {
    case 'alpha':
    case 'beta':
    case 'stable':
      return configuredChannel
  }

  return releaseTag?.match(/-alpha\./)
    ? 'alpha'
    : releaseTag?.match(/-beta\./)
      ? 'beta'
      : releaseTag?.startsWith('v')
        ? 'stable'
        : 'dev'
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
    react({ compiler: true }),
    tailwindcss(),
  ],
  build: {
    manifest: true,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [{ name: 'initial', tags: ['$initial'] }],
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
        target: process.env.OORED_URL || 'http://127.0.0.1:8787',
        changeOrigin: true,
        ws: true,
      },
    },
  },
})
