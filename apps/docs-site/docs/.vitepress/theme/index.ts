import DefaultTheme from 'vitepress/theme'
import { theme, useOpenapi } from 'vitepress-openapi/client'
import 'vitepress-openapi/dist/style.css'
import { useData, useRoute } from 'vitepress'
import { h, onMounted, onUnmounted, watchEffect } from 'vue'
import spec from '../../public/openapi.json' with { type: 'json' }
import './custom.css'
import type { Theme } from 'vitepress'

function injectStructuredData() {
  const route = useRoute()
  const { frontmatter, title, description } = useData()

  onMounted(() => {
    const update = () => {
      // Remove any existing structured data we injected
      document
        .querySelectorAll('script[data-seo="oore"]')
        .forEach((el) => el.remove())

      const path = route.path
      const segments = path.split('/').filter(Boolean)
      const hostname = 'https://docs.oore.build'

      // BreadcrumbList
      const breadcrumbs = [
        { name: 'Docs', url: hostname + '/' },
        ...segments.map((seg, i) => ({
          name: seg
            .split('-')
            .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
            .join(' '),
          url: hostname + '/' + segments.slice(0, i + 1).join('/'),
        })),
      ]

      const breadcrumbLd = {
        '@context': 'https://schema.org',
        '@type': 'BreadcrumbList',
        itemListElement: breadcrumbs.map((bc, i) => ({
          '@type': 'ListItem',
          position: i + 1,
          name: bc.name,
          item: bc.url,
        })),
      }

      // TechArticle for content pages (not the home page)
      const scripts = [breadcrumbLd]
      if (segments.length > 0) {
        scripts.push({
          '@context': 'https://schema.org',
          '@type': 'TechArticle',
          headline: title.value || 'Oore CI Docs',
          description:
            description.value ||
            'Documentation for oore.build, a self-hosted Flutter-first mobile CI platform.',
          url: hostname + path,
          author: {
            '@type': 'Person',
            name: 'Aryakumar Jha',
            url: 'https://aryak.dev',
            sameAs: 'https://github.com/devaryakjha',
          },
        } as Record<string, unknown>)
      }

      scripts.forEach((data) => {
        const script = document.createElement('script')
        script.type = 'application/ld+json'
        script.dataset.seo = 'oore'
        script.textContent = JSON.stringify(data)
        document.head.appendChild(script)
      })
    }

    // Watch for route changes
    const stop = watchEffect(update)
    onUnmounted(() => stop())
  })

  return null
}

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    const openapi = useOpenapi({
      spec,
      config: {
        server: {
          allowCustomServer: true,
        },
      },
    })
    theme.enhanceApp({ app, openapi })
  },
  Layout() {
    return h(DefaultTheme.Layout, null, {
      'layout-top': () => h(injectStructuredData),
    })
  },
} satisfies Theme
