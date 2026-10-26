import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { RouterProvider, createRouter } from '@tanstack/react-router'
import { useHotkey } from '@tanstack/react-hotkeys'
import { ThemeProvider, useTheme } from 'next-themes'

// Import the generated route tree
import { routeTree } from './routeTree.gen'

import { Toaster } from '@/components/ui/sonner'
import { SHORTCUTS, shortcutOptions } from '@/lib/keyboard-shortcuts'

import './fonts.css'
import './styles.css'

function ThemeHotkey() {
  const { resolvedTheme, setTheme } = useTheme()

  useHotkey(
    SHORTCUTS.toggleTheme.hotkey,
    () => setTheme(resolvedTheme === 'dark' ? 'light' : 'dark'),
    shortcutOptions(SHORTCUTS.toggleTheme),
  )

  return null
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

function dismissAppLoading() {
  const loading = document.getElementById('app-loading')
  if (!loading) return

  const exitLoading = () => {
    const removeLoading = (event: AnimationEvent) => {
      if (event.target === loading) {
        loading.remove()
      }
    }

    loading.addEventListener('animationend', removeLoading)
    requestAnimationFrame(() => {
      loading.classList.add('app-loading--exit')
    })
  }

  const mark = loading.querySelector<SVGElement>('.app-loading__mark')
  const assemblyAnimation = mark?.getAnimations()[0]

  if (assemblyAnimation && assemblyAnimation.playState !== 'finished') {
    void assemblyAnimation.finished.then(exitLoading, exitLoading)
    return
  }

  exitLoading()
}

// Register the router instance for type safety
declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>
  }
  interface StaticDataRouteOption {
    breadcrumb?: {
      title: string
      /** False when the route only redirects, so its crumb is plain text. */
      link?: boolean
    }
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
        <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
          <ThemeHotkey />
          <RouterProvider router={router} />
          <Toaster />
        </ThemeProvider>
      </StrictMode>,
    )
    dismissAppLoading()
  }
}

void boot()
