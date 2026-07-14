import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { RouterProvider, createRouter } from '@tanstack/react-router'

// Import the generated route tree
import { routeTree } from './routeTree.gen'

import './styles.css'
import reportWebVitals from './reportWebVitals.ts'

function createAppRouter() {
  return createRouter({
    routeTree,
    context: {},
    defaultPreload: 'intent',
    scrollRestoration: true,
    defaultStructuralSharing: true,
    defaultPreloadStaleTime: 0,
  })
}

// Register the router instance for type safety
declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>
  }
  interface StaticDataRouteOption {
    breadcrumbLabel?: string
    breadcrumbParent?: { label: string; to: string }
  }
}

// Boot the app — conditionally enables demo mode before rendering
async function boot() {
  if (import.meta.env.VITE_DEMO_MODE === 'true') {
    const { enableDemoMode } = await import('./demo/enable-demo')
    await enableDemoMode()
  }

  // Create the router only after optional demo bootstrapping.
  // Some routes read local/session storage in `beforeLoad` guards,
  // so demo seeding must happen first to support deep links.
  const router = createAppRouter()

  const rootElement = document.getElementById('app')
  if (rootElement && !rootElement.dataset.reactRoot) {
    rootElement.dataset.reactRoot = 'true'
    const root = ReactDOM.createRoot(rootElement)
    root.render(
      <StrictMode>
        <RouterProvider router={router} />
      </StrictMode>,
    )
  }
}

void boot()

// Report Core Web Vitals to console in development
reportWebVitals(import.meta.env.DEV ? console.log : undefined)
