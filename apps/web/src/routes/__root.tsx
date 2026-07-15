import { Suspense, lazy } from 'react'
import {
  Outlet,
  createRootRoute,
  useMatches,
  useRouterState,
} from '@tanstack/react-router'
import { QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'
import { HugeiconsIcon } from '@hugeicons/react'
import { Search01Icon } from '@hugeicons/core-free-icons'

import AppSidebar from '@/components/app-sidebar'
import ConnectivityBanner from '@/components/connectivity-banner'
import PageBreadcrumb from '@/components/page-breadcrumb'
import QaAppHeader from '@/components/qa-app-header'
import { Separator } from '@/components/ui/separator'
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@/components/ui/sidebar'
import { Kbd } from '@/components/ui/kbd'
import { Toaster } from '@/components/ui/sonner'
import { useSessionMonitor } from '@/hooks/use-session-monitor'
import { syncSetupStoreContext } from '@/lib/instance-context'
import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { useUiStore } from '@/stores/ui-store'
import { useInstanceStore } from '@/stores/instance-store'
import { isDemoMode } from '@/lib/demo-mode'
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

function RouteTransitionBar() {
  const isLoading = useRouterState({
    select: (s) => s.status === 'pending',
  })
  if (!isLoading) return null
  return (
    <div className="fixed top-0 left-0 right-0 z-50 h-0.5 overflow-hidden bg-primary/20">
      <div className="h-full w-1/3 animate-[route-slide_1s_ease-in-out_infinite] bg-primary" />
    </div>
  )
}

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
        <RouteTransitionBar />
        {showQaChrome ? (
          <div className="flex min-h-screen flex-col bg-surface">
            <QaAppHeader />
            <ConnectivityBanner />
            <main className="flex flex-1 flex-col">
              <Outlet />
            </main>
          </div>
        ) : showAppChrome ? (
          <SidebarProvider open={sidebarOpen} onOpenChange={setSidebarOpen}>
            <AppSidebar />
            <SidebarInset>
              <header className="sticky top-0 z-30 flex h-12 shrink-0 items-center gap-2 border-b bg-background px-4">
                <SidebarTrigger className="-ml-1" />
                <Separator
                  orientation="vertical"
                  className="mr-2 h-4! self-auto!"
                />
                <PageBreadcrumb />
                <div className="ml-auto">
                  <button
                    type="button"
                    aria-label="Search"
                    onMouseEnter={() => void loadCommandPalette()}
                    onFocus={() => void loadCommandPalette()}
                    onClick={() => openCommandPalette(true)}
                    className="inline-flex h-8 w-8 items-center justify-center gap-2 rounded-sm border bg-muted/50 px-0 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground sm:w-48 sm:justify-between sm:px-3"
                  >
                    <HugeiconsIcon icon={Search01Icon} size={14} />
                    <span className="hidden sm:inline">Search</span>
                    <Kbd className="hidden sm:inline-flex">⌘K</Kbd>
                  </button>
                </div>
              </header>
              <ConnectivityBanner />
              <div className="flex flex-1 flex-col bg-surface">
                <Outlet />
              </div>
            </SidebarInset>
          </SidebarProvider>
        ) : (
          <div className="min-h-screen flex flex-col bg-surface">
            <ConnectivityBanner />
            <div className="flex-1 flex flex-col">
              <Outlet />
            </div>
          </div>
        )}
        <Toaster />
        {showAppChrome && !showQaChrome && commandPaletteOpen ? (
          <Suspense fallback={null}>
            <CommandPalette />
          </Suspense>
        ) : null}
        {isDemoMode && (
          <div className="fixed bottom-4 right-4 z-50 flex items-center gap-2 border bg-background px-3 py-1.5 text-xs text-muted-foreground shadow-md">
            <span className="size-1.5 animate-pulse bg-primary" />
            Demo Mode
            <span className="text-muted-foreground/50">—</span>
            <a
              href="https://oore.build"
              className="text-primary underline underline-offset-2"
              target="_blank"
              rel="noopener noreferrer"
            >
              View Oore product details
            </a>
          </div>
        )}
        <Suspense>
          <DevTools />
        </Suspense>
      </QueryClientProvider>
    </ThemeProvider>
  )
}
