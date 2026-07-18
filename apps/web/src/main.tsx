import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { RouterProvider, createRouter } from '@tanstack/react-router'

// Import the generated route tree
import { routeTree } from './routeTree.gen'

import './styles.css'

function reportRenderError() {
  void import('./web-performance')
    .then(({ reportWebRenderError }) => reportWebRenderError())
    .catch(() => {
      // Reliability telemetry is best-effort and contains no error details.
    })
}

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
    breadcrumb?: {
      title: string;
    }
  }
}

// Boot the app — conditionally enables demo mode before rendering
async function boot() {
  try {
    const storedComponentStyle = localStorage.getItem('oore_component_style')
    const { applyComponentStyle, loadComponentStyle } =
      await import('./lib/component-style')
    await loadComponentStyle(storedComponentStyle)
    applyComponentStyle(storedComponentStyle)

    const storedTheme = localStorage.getItem('oore_color_theme')
    if (storedTheme && storedTheme !== 'amber') {
      const { applyColorTheme } = await import('./lib/color-theme')
      applyColorTheme(storedTheme)
    }
  } catch {
    // Keep the default Amber theme.
  }

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
    const root = ReactDOM.createRoot(rootElement, {
      onCaughtError: reportRenderError,
      onUncaughtError: reportRenderError,
    })
    root.render(
      <StrictMode>
        <RouterProvider router={router} />
      </StrictMode>,
    )
  }
}

void boot()

const startPerformanceMonitoring = () => {
  void import('./web-performance')
    .then(({ startWebPerformanceMonitoring }) =>
      startWebPerformanceMonitoring(),
    )
    .catch(() => {
      // Monitoring is best-effort and must not affect app startup.
    })
}

const requestIdleCallback = (
  window as unknown as {
    requestIdleCallback?: Window['requestIdleCallback']
  }
).requestIdleCallback
if (requestIdleCallback) {
  requestIdleCallback(startPerformanceMonitoring, { timeout: 2_000 })
} else {
  setTimeout(startPerformanceMonitoring, 0)
}
