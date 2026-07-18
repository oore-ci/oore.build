import DefaultTheme from 'vitepress/theme'
import { useData, useRoute } from 'vitepress'
import { h, onMounted, onUnmounted, watchEffect } from 'vue'
import type { EnhanceAppContext, Theme } from 'vitepress'
import './custom.css'

const openApiApps = new WeakSet<EnhanceAppContext['app']>()

function isOpenApiPath(path: string) {
  return path === '/openapi' || path.startsWith('/openapi/')
}

async function enableOpenApi(context: EnhanceAppContext) {
  if (openApiApps.has(context.app)) return

  const [{ theme, useOpenapi }, { default: spec }] = await Promise.all([
    import('vitepress-openapi/client'),
    import('../../public/openapi.json'),
    import('vitepress-openapi/dist/style.css'),
  ])

  const openapi = useOpenapi({
    spec,
    config: {
      server: { allowCustomServer: true },
    },
  })

  theme.enhanceApp({ ...context, openapi })
  openApiApps.add(context.app)
}

function StructuredData() {
  const route = useRoute()
  const { title, description } = useData()

  onMounted(() => {
    const update = () => {
      document
        .querySelectorAll('script[data-seo="oore"]')
        .forEach((element) => element.remove())

      const segments = route.path.split('/').filter(Boolean)
      const origin = 'https://docs.oore.build'
      const breadcrumbs = [
        { name: 'Docs', url: `${origin}/` },
        ...segments.map((segment, index) => ({
          name: segment
            .split('-')
            .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
            .join(' '),
          url: `${origin}/${segments.slice(0, index + 1).join('/')}`,
        })),
      ]

      const scripts: Array<Record<string, unknown>> = [
        {
          '@context': 'https://schema.org',
          '@type': 'BreadcrumbList',
          itemListElement: breadcrumbs.map((breadcrumb, index) => ({
            '@type': 'ListItem',
            position: index + 1,
            name: breadcrumb.name,
            item: breadcrumb.url,
          })),
        },
      ]

      if (segments.length > 0) {
        scripts.push({
          '@context': 'https://schema.org',
          '@type': 'TechArticle',
          headline: title.value || 'Oore CI docs',
          description:
            description.value ||
            'Documentation for installing, operating, and integrating Oore CI.',
          url: `${origin}${route.path}`,
          author: {
            '@type': 'Organization',
            name: 'oore.build',
            url: 'https://oore.build',
          },
        })
      }

      for (const data of scripts) {
        const script = document.createElement('script')
        script.type = 'application/ld+json'
        script.dataset.seo = 'oore'
        script.textContent = JSON.stringify(data)
        document.head.appendChild(script)
      }
    }

    const stop = watchEffect(update)
    onUnmounted(stop)
  })

  return null
}

export default {
  extends: DefaultTheme,

  async enhanceApp(context) {
    if (isOpenApiPath(context.router.route.path)) {
      await enableOpenApi(context)
    }

    const previousBeforePageLoad = context.router.onBeforePageLoad
    context.router.onBeforePageLoad = async (to) => {
      if (isOpenApiPath(to)) await enableOpenApi(context)
      return previousBeforePageLoad?.(to)
    }
  },

  Layout() {
    return h(DefaultTheme.Layout, null, {
      'layout-top': () => h(StructuredData),
    })
  },
} satisfies Theme
