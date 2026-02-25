import { render } from 'solid-js/web'
import { RouterProvider, createRouter } from '@tanstack/solid-router'
import { routeTree } from './routeTree.gen'
import './styles.css'
import reportWebVitals from './reportWebVitals'

function createAppRouter() {
  return createRouter({
    routeTree,
    context: {},
    defaultPreload: 'intent',
    defaultPreloadDelay: 50,
    defaultPreloadStaleTime: 0,
    defaultPendingMs: 150,
    defaultPendingMinMs: 200,
    scrollRestoration: true,
    defaultStructuralSharing: true,
  })
}

declare module '@tanstack/solid-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>
  }
  interface StaticDataRouteOption {
    breadcrumbLabel?: string
  }
}

async function boot() {
  if (import.meta.env.VITE_DEMO_MODE === 'true') {
    const { enableDemoMode } = await import('./demo/enable-demo')
    await enableDemoMode()
  }

  const router = createAppRouter()
  const rootElement = document.getElementById('app')
  if (!rootElement) return
  // index.html ships a static loading shell for first paint.
  // Solid mounts alongside existing DOM, so clear it explicitly.
  rootElement.replaceChildren()

  render(() => <RouterProvider router={router} />, rootElement)
}

void boot()
reportWebVitals(import.meta.env.DEV ? console.log : undefined)
