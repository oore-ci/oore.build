import { Outlet, createRootRoute, useMatches } from '@tanstack/react-router'
import { QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'

import AppSidebar from '@/components/app-sidebar'
import ConnectivityBanner from '@/components/connectivity-banner'
import DeferredToaster from '@/components/deferred-toaster'
import QaAppHeader from '@/components/qa-app-header'
import RouteTransitionBar from '@/components/route-transition-bar'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { useSessionMonitor } from '@/hooks/use-session-monitor'
import { syncSetupStoreContext } from '@/lib/instance-context'
import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import {
  RootErrorBoundary,
  RootNotFound,
} from '@/components/root-route-boundaries'
import SiteHeader from '@/components/site-header'

export const Route = createRootRoute({
  beforeLoad: () => {
    const activeId = useInstanceStore.getState().activeInstanceId
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

  // Show sidebar+header only when: not on setup/login AND instance+auth exist
  const showAppChrome =
    !isSetupRoute &&
    !isLoginRoute &&
    !!activeInstanceId &&
    !!authToken &&
    !!authUser
  const showQaChrome = showAppChrome && authUser.role === 'qa_viewer'

  useSessionMonitor()

  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <QueryClientProvider client={queryClient}>
        {/* TODO: bring this back once a concrete list of hotkeys for the app is decided */}
        {/* <AppShortcuts />  */}
        <a
          href="#main-content"
          className="fixed top-2 left-2 z-100 -translate-y-20 px-3 py-2 text-sm font-medium text-foreground ring-2 ring-ring transition-transform focus:translate-y-0"
        >
          Skip to content
        </a>
        <RouteTransitionBar />
        {showQaChrome ? (
          <div className="flex min-h-dvh flex-col">
            <QaAppHeader />
            <ConnectivityBanner />
            <main
              id="main-content"
              tabIndex={-1}
              className="flex flex-1 flex-col"
            >
              <div className="@container/main flex flex-1 flex-col gap-2">
                <Outlet />
              </div>
            </main>
          </div>
        ) : showAppChrome ? (
          <SidebarProvider
            style={
              {
                '--sidebar-width': 'calc(var(--spacing) * 72)',
                '--header-height': 'calc(var(--spacing) * 12)',
              } as React.CSSProperties
            }
          >
            <AppSidebar variant="inset" />
            <SidebarInset id="main-content" tabIndex={-1}>
              <SiteHeader />
              <ConnectivityBanner />
              <div className="flex flex-1 flex-col">
                <div className="@container/main flex flex-1 flex-col gap-2">
                  <Outlet />
                </div>
              </div>
            </SidebarInset>
          </SidebarProvider>
        ) : (
          <main
            id="main-content"
            tabIndex={-1}
            className="flex min-h-dvh flex-col pt-(--safe-area-top) pb-(--safe-area-bottom)"
          >
            <ConnectivityBanner />
            <div className="flex flex-1 flex-col">
              <div className="@container/main flex flex-1 flex-col gap-2">
                <Outlet />
              </div>
            </div>
          </main>
        )}
        <DeferredToaster />
      </QueryClientProvider>
    </ThemeProvider>
  )
}
