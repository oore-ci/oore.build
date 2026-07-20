import { Suspense, lazy } from 'react'
import { Outlet, createRootRoute, useMatches } from '@tanstack/react-router'
import { QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Search as Search01Icon } from 'lucide-react'

import AppSidebar from '@/components/app-sidebar'
import ConnectivityBanner from '@/components/connectivity-banner'
import DeferredToaster from '@/components/deferred-toaster'
import PageBreadcrumb from '@/components/page-breadcrumb'
import QaAppHeader from '@/components/qa-app-header'
import RouteTransitionBar from '@/components/route-transition-bar'
import ThemeColorSync from '@/components/theme-color-sync'
import { Separator } from '@/components/ui/separator'
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@/components/ui/sidebar'
import { Kbd } from '@/components/ui/kbd'
import { Button } from '@/components/ui/button'
import { useSessionMonitor } from '@/hooks/use-session-monitor'
import { syncSetupStoreContext } from '@/lib/instance-context'
import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { useUiStore } from '@/stores/ui-store'
import { useInstanceStore } from '@/stores/instance-store'
import { useWindowEvent } from '@/hooks/use-window-event'
import {
  RootErrorBoundary,
  RootNotFound,
} from '@/components/root-route-boundaries'

const loadCommandPalette = () => import('@/components/command-palette')
const CommandPalette = lazy(loadCommandPalette)

const DevTools = import.meta.env.DEV
  ? lazy(() =>
      Promise.all([
        import('@tanstack/react-devtools'),
        import('@tanstack/react-router-devtools'),
      ]).then(([devMod, routerDevMod]) => ({
        default: () => (
          <devMod.TanStackDevtools
            config={{ position: 'bottom-right' }}
            plugins={[
              {
                name: 'Tanstack Router',
                render: <routerDevMod.TanStackRouterDevtoolsPanel />,
              },
            ]}
          />
        ),
      })),
    )
  : () => null

export const Route = createRootRoute({
  beforeLoad: () => {
    let activeId = useInstanceStore.getState().activeInstanceId
    if (!activeId) {
      try {
        const raw = localStorage.getItem('oore_instances')
        if (raw) {
          const parsed = JSON.parse(raw) as {
            state?: { activeInstanceId?: string | null }
          }
          activeId = parsed.state?.activeInstanceId ?? null
        }
      } catch {
        // ignore
      }
    }
    if (activeId) {
      syncSetupStoreContext(activeId)
      useAuthStore.getState().setInstanceContext(activeId)
    }
  },
  component: RootLayout,
  notFoundComponent: RootNotFound,
  errorComponent: RootErrorBoundary,
})

function RootLayout() {
  const matches = useMatches()
  const isSetupRoute = matches.some((m) => m.fullPath.startsWith('/setup'))
  const isLoginRoute = matches.some(
    (m) => m.fullPath.startsWith('/login') || m.fullPath.startsWith('/auth'),
  )
  const activeInstanceId = useInstanceStore((s) => s.activeInstanceId)
  const authToken = useAuthStore((s) => s.token)
  const authUser = useAuthStore((s) => s.user)
  const openCommandPalette = useUiStore((s) => s.setCommandPaletteOpen)
  const commandPaletteOpen = useUiStore((s) => s.commandPaletteOpen)
  const toggleCommandPalette = useUiStore((s) => s.toggleCommandPalette)
  const sidebarOpen = useUiStore((s) => s.sidebarOpen)
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen)

  // Show sidebar+header only when: not on setup/login AND instance+auth exist
  const showAppChrome =
    !isSetupRoute &&
    !isLoginRoute &&
    !!activeInstanceId &&
    !!authToken &&
    !!authUser
  const showQaChrome = showAppChrome && authUser.role === 'qa_viewer'

  useSessionMonitor()
  useWindowEvent('keydown', (event) => {
    if (
      showAppChrome &&
      !showQaChrome &&
      (event.metaKey || event.ctrlKey) &&
      event.key === 'k'
    ) {
      event.preventDefault()
      void loadCommandPalette()
      toggleCommandPalette()
    }
  })

  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <QueryClientProvider client={queryClient}>
        <ThemeColorSync />
        <a
          href="#main-content"
          className="fixed top-2 left-2 z-100 -translate-y-20 px-3 py-2 text-sm font-medium text-foreground ring-2 ring-ring transition-transform focus:translate-y-0"
        >
          Skip to content
        </a>
        <RouteTransitionBar />
        {showQaChrome ? (
          <div className="flex min-h-dvh flex-col bg-surface">
            <QaAppHeader />
            <ConnectivityBanner />
            <main
              id="main-content"
              tabIndex={-1}
              className="flex flex-1 flex-col"
            >
              <Outlet />
            </main>
          </div>
        ) : showAppChrome ? (
          <SidebarProvider open={sidebarOpen} onOpenChange={setSidebarOpen}>
            <AppSidebar />
            <SidebarInset
              id="main-content typeset typeset-docs max-w-[37em]"
              tabIndex={-1}
            >
              <header className="sticky top-0 z-30 flex h-[calc(3rem+var(--safe-area-top))] shrink-0 items-center gap-2 border-b px-4 pt-(--safe-area-top)">
                <SidebarTrigger className="-ml-1" />
                <Separator
                  orientation="vertical"
                  className="mr-2 h-4! self-auto!"
                />
                <PageBreadcrumb />
                <div className="ml-auto">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    aria-label="Search"
                    onMouseEnter={() => void loadCommandPalette()}
                    onFocus={() => void loadCommandPalette()}
                    onClick={() => openCommandPalette(true)}
                    className="w-10 justify-center px-0 text-xs text-muted-foreground sm:w-52 sm:justify-between sm:px-3"
                  >
                    <DynamicLucideIcon icon={Search01Icon} size={14} />
                    <span className="hidden sm:inline">Search</span>
                    <Kbd className="hidden sm:inline-flex">⌘K</Kbd>
                  </Button>
                </div>
              </header>
              <ConnectivityBanner />
              <div className="flex flex-1 flex-col bg-surface">
                <Outlet />
              </div>
            </SidebarInset>
          </SidebarProvider>
        ) : (
          <main
            id="main-content"
            tabIndex={-1}
            className="flex min-h-dvh flex-col bg-surface pt-(--safe-area-top) pb-(--safe-area-bottom)"
          >
            <ConnectivityBanner />
            <div className="flex flex-1 flex-col">
              <Outlet />
            </div>
          </main>
        )}
        <DeferredToaster />
        {showAppChrome && !showQaChrome && commandPaletteOpen ? (
          <Suspense fallback={null}>
            <CommandPalette />
          </Suspense>
        ) : null}
        <Suspense>
          <DevTools />
        </Suspense>
      </QueryClientProvider>
    </ThemeProvider>
  )
}
