import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { URL, fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'
import viteReact from '@vitejs/plugin-react'

import tailwindcss from '@tailwindcss/vite'

import { tanstackRouter } from '@tanstack/router-plugin/vite'

import type { Plugin } from 'vite'

/**
 * Post-build plugin that optimises the production HTML:
 * 1. Preloads the primary font (Google Sans Flex latin) to eliminate FOUT.
 * 2. Removes modulepreload for deferred chunks (form-vendor) that are
 *    lazy-loaded on interaction and shouldn't block the critical path.
 */
function htmlOptimisePlugin(): Plugin {
  return {
    name: 'html-optimise',
    apply: 'build',
    enforce: 'post',
    closeBundle() {
      const outDir = 'dist'
      const htmlPath = `${outDir}/index.html`

      let html: string
      try {
        html = readFileSync(htmlPath, 'utf-8')
      } catch {
        return
      }

      // 1. Remove modulepreload for lazy-loaded chunks
      html = html.replace(
        /<link rel="modulepreload"[^>]*href="[^"]*form-vendor[^"]*"[^>]*>\n?/g,
        '',
      )

      // 2. Find the hashed font filename and inject a preload hint
      const assetFiles = readdirSync(`${outDir}/assets`)
      const fontFile = assetFiles.find(
        (f) =>
          f.startsWith('google-sans-flex-latin-') &&
          f.endsWith('.woff2') &&
          !f.includes('ext'),
      )
      if (fontFile) {
        const preload = `    <link rel="preload" href="/assets/${fontFile}" as="font" type="font/woff2" crossorigin>\n`
        html = html.replace(
          /(\s*<link rel="stylesheet")/,
          `\n${preload}$1`,
        )
      }

      writeFileSync(htmlPath, html)
    },
  }
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    devtools(),
    tanstackRouter({
      target: 'react',
      autoCodeSplitting: true,
    }),
    viteReact(),
    tailwindcss(),
    htmlOptimisePlugin(),
  ],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) return

          // React core + UI primitives + Router in one chunk to prevent
          // circular chunk deps (Base UI ↔ React, Router ↔ React share
          // module boundaries that cause TDZ errors when split)
          if (
            id.includes('/react-dom/') ||
            id.includes('/react/') ||
            id.includes('/scheduler/') ||
            id.includes('/@base-ui/') ||
            id.includes('/@floating-ui/') ||
            id.includes('/tabbable/') ||
            id.includes('/@tanstack/react-router/') ||
            id.includes('/@tanstack/router-') ||
            id.includes('/@tanstack/history') ||
            id.includes('/@tanstack/react-store') ||
            id.includes('/@tanstack/store') ||
            id.includes('/tiny-invariant/') ||
            id.includes('/tiny-warning/')
          )
            return 'react-vendor'

          // TanStack Query (no circular dep with react-vendor)
          if (id.includes('/@tanstack/react-query/') || id.includes('/@tanstack/query-core/'))
            return 'query-vendor'

          // Form libs (lazy-loaded via dialog components)
          if (id.includes('/react-hook-form/') || id.includes('/zod/') || id.includes('/@hookform/'))
            return 'form-vendor'

          // Toast notifications
          if (id.includes('/sonner/'))
            return 'sonner'

          // Icon library
          if (id.includes('/@hugeicons/'))
            return 'icons'

          // Styling utilities (pure functions, no React)
          if (id.includes('/tailwind-merge/') || id.includes('/class-variance-authority/') || id.includes('/clsx/'))
            return 'ui-utils'
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
        target: 'http://127.0.0.1:8787',
        changeOrigin: true,
      },
    },
  },
})
