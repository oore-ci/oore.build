import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { URL, fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'
import viteReact from '@vitejs/plugin-react'

import tailwindcss from '@tailwindcss/vite'

import { tanstackRouter } from '@tanstack/router-plugin/vite'

import type { Plugin } from 'vite'

/**
 * Post-build plugin that generates sitemap.xml from the public routes list.
 * For an SPA behind auth, only explicitly-listed public routes are included.
 * This runs at build time so the sitemap always stays in sync with deploys.
 */
function sitemapPlugin(): Plugin {
  const HOSTNAME = 'https://ci.oore.build'
  const PUBLIC_ROUTES: Array<{
    path: string
    changefreq: string
    priority: string
  }> = [
    { path: '/', changefreq: 'weekly', priority: '1.0' },
    { path: '/login', changefreq: 'monthly', priority: '0.5' },
  ]

  return {
    name: 'generate-sitemap',
    apply: 'build',
    enforce: 'post',
    closeBundle() {
      const outDir = 'dist'
      const urls = PUBLIC_ROUTES.map(
        (r) =>
          `  <url>\n    <loc>${HOSTNAME}${r.path}</loc>\n    <changefreq>${r.changefreq}</changefreq>\n    <priority>${r.priority}</priority>\n  </url>`,
      ).join('\n')

      const sitemap = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`
      writeFileSync(`${outDir}/sitemap.xml`, sitemap)
    },
  }
}

/**
 * Post-build plugin that optimises the production HTML:
 * 1. Preloads the primary font (Google Sans Flex latin) to eliminate FOUT.
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

      // Find the hashed font filename and inject a preload hint
      const assetFiles = readdirSync(`${outDir}/assets`)
      const fontFile = assetFiles.find(
        (f) =>
          f.startsWith('google-sans-flex-latin-') &&
          f.endsWith('.woff2') &&
          !f.includes('ext'),
      )
      if (fontFile) {
        const preload = `    <link rel="preload" href="/assets/${fontFile}" as="font" type="font/woff2" crossorigin>\n`
        html = html.replace(/(\s*<link rel="stylesheet")/, `\n${preload}$1`)
      }

      writeFileSync(htmlPath, html)
    },
  }
}

// https://vitejs.dev/config/
export default defineConfig({
  define: {
    // Enable demo mode by default for local dev when no real backend is configured.
    // Set VITE_DEMO_MODE=false in your .env.local to disable.
    'import.meta.env.VITE_DEMO_MODE': JSON.stringify(
      process.env.VITE_DEMO_MODE ?? 'true',
    ),
    'import.meta.env.VITE_RELEASE_CHANNEL': JSON.stringify(
      process.env.OORE_WEB_RELEASE_CHANNEL === 'alpha' ||
        process.env.OORE_WEB_RELEASE_CHANNEL === 'beta' ||
        process.env.OORE_WEB_RELEASE_CHANNEL === 'stable'
        ? process.env.OORE_WEB_RELEASE_CHANNEL
        : process.env.RELEASE_TAG?.includes('-alpha.')
          ? 'alpha'
          : process.env.RELEASE_TAG?.includes('-beta.')
            ? 'beta'
            : process.env.RELEASE_TAG?.startsWith('v')
              ? 'stable'
              : 'dev',
    ),
  },
  plugins: [
    devtools(),
    tanstackRouter({
      target: 'react',
      autoCodeSplitting: true,
    }),
    viteReact(),
    tailwindcss(),
    htmlOptimisePlugin(),
    sitemapPlugin(),
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
            id.includes('/@tanstack/react-store') ||
            id.includes('/@tanstack/store') ||
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
