import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { URL, fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import solid from 'vite-plugin-solid'
import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'

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
      mkdirSync(outDir, { recursive: true })
      const urls = PUBLIC_ROUTES.map(
        (route) =>
          `  <url>\n    <loc>${HOSTNAME}${route.path}</loc>\n    <changefreq>${route.changefreq}</changefreq>\n    <priority>${route.priority}</priority>\n  </url>`,
      ).join('\n')

      const sitemap = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`
      writeFileSync(`${outDir}/sitemap.xml`, sitemap)
    },
  }
}

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

      const assetFiles = readdirSync(`${outDir}/assets`)
      const fontFile = assetFiles.find(
        (fileName) =>
          fileName.startsWith('google-sans-flex-latin-') &&
          fileName.endsWith('.woff2') &&
          !fileName.includes('ext'),
      )
      if (fontFile) {
        const preload = `    <link rel="preload" href="/assets/${fontFile}" as="font" type="font/woff2" crossorigin>\n`
        html = html.replace(/(\s*<link rel="stylesheet")/, `\n${preload}$1`)
      }

      writeFileSync(htmlPath, html)
    },
  }
}

export default defineConfig({
  plugins: [
    tanstackRouter({
      target: 'solid',
      autoCodeSplitting: true,
    }),
    solid(),
    tailwindcss(),
    htmlOptimisePlugin(),
    sitemapPlugin(),
  ],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) return

          if (
            id.includes('/solid-js/') ||
            id.includes('/@tanstack/solid-router/') ||
            id.includes('/@tanstack/router-')
          )
            return 'solid-vendor'

          if (
            id.includes('/@tanstack/solid-query/') ||
            id.includes('/@tanstack/query-core/')
          )
            return 'query-vendor'

          if (id.includes('/@tanstack/solid-form/') || id.includes('/zod/'))
            return 'form-vendor'

          if (id.includes('/@hugeicons/')) return 'icons'

          if (
            id.includes('/tailwind-merge/') ||
            id.includes('/class-variance-authority/') ||
            id.includes('/clsx/')
          )
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
