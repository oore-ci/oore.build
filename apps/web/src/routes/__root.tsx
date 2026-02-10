import { Suspense, lazy, useEffect } from 'react'
import {
  Link,
  Outlet,
  createRootRoute,
  useMatches,
} from '@tanstack/react-router'
import { QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'

import AppSidebar from '@/components/app-sidebar'
import PageBreadcrumb from '@/components/page-breadcrumb'
import { Separator } from '@/components/ui/separator'
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@/components/ui/sidebar'
import { Toaster } from '@/components/ui/sonner'
import { syncSetupStoreContext } from '@/lib/instance-context'
import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import { useSetupStore } from '@/stores/setup-store'

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
})

function RootLayout() {
  const matches = useMatches()
  const isSetupRoute = matches.some((m) => m.fullPath.startsWith('/setup'))
  const isLoginRoute = matches.some(
    (m) =>
      m.fullPath.startsWith('/login') || m.fullPath.startsWith('/auth'),
  )
  const showSidebar = !isSetupRoute && !isLoginRoute
  const activeInstanceId = useInstanceStore((s) => s.activeInstanceId)

  useEffect(() => {
    useSetupStore.getState().setInstanceContext(activeInstanceId)
    useAuthStore.getState().setInstanceContext(activeInstanceId)
  }, [activeInstanceId])

  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <QueryClientProvider client={queryClient}>
        {showSidebar ? (
          <SidebarProvider>
            <AppSidebar />
            <SidebarInset>
              <header className="sticky top-0 z-30 flex h-12 shrink-0 items-center gap-2 border-b bg-background px-4">
                <SidebarTrigger className="-ml-1" />
                <Separator orientation="vertical" className="mr-2 h-4! self-auto!" />
                <Link to="/" className="flex items-center gap-2 pr-1">
                  <img src="/logo.svg" alt="oore.build logo" className="size-5" />
                  <span className="hidden text-sm font-semibold tracking-tight sm:inline">
                    oore.build
                  </span>
                </Link>
                <Separator orientation="vertical" className="mr-2 h-4! self-auto!" />
                <PageBreadcrumb />
              </header>
              <div className="flex flex-1 flex-col bg-surface">
                <Outlet />
              </div>
            </SidebarInset>
          </SidebarProvider>
        ) : (
          <div className="min-h-screen flex flex-col bg-surface">
            <div className="flex-1 flex flex-col">
              <Outlet />
            </div>
          </div>
        )}
        <Toaster />
        <Suspense>
          <DevTools />
        </Suspense>
      </QueryClientProvider>
    </ThemeProvider>
  )
}
