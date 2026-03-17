import { Suspense, lazy, useEffect } from 'react'
import {
  Link,
  Outlet,
  createRootRoute,
  useMatches,
  useRouter,
  useRouterState,
} from '@tanstack/react-router'
import { QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  ArrowLeft02Icon,
  Home01Icon,
  RotateClockwiseIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'
import type { ErrorComponentProps } from '@tanstack/react-router'

import AppSidebar from '@/components/app-sidebar'
const CommandPalette = lazy(() => import('@/components/command-palette'))
import ConnectivityBanner from '@/components/connectivity-banner'
import PageBreadcrumb from '@/components/page-breadcrumb'
import { Button } from '@/components/ui/button'
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

function NotFound() {
  return (
    <div className="flex min-h-[60vh] flex-col items-center justify-center gap-6 px-6 text-center">
      <div className="space-y-2">
        <p className="text-6xl font-bold tracking-tight text-muted-foreground/40">
          404
        </p>
        <h1 className="text-2xl font-bold tracking-tight">Page not found</h1>
        <p className="text-sm text-muted-foreground">
          The page you're looking for doesn't exist or has been moved.
        </p>
      </div>
      <div className="flex items-center gap-3">
        <Button variant="outline" render={<Link to="/" />} nativeButton={false}>
          <HugeiconsIcon icon={Home01Icon} size={16} />
          Dashboard
        </Button>
      </div>
    </div>
  )
}

function RootError({ error, reset }: ErrorComponentProps) {
  const router = useRouter()
  return (
    <div className="flex min-h-[60vh] flex-col items-center justify-center gap-6 px-6 text-center">
      <div className="flex size-12 items-center justify-center border border-destructive/30 bg-destructive/10">
        <HugeiconsIcon
          icon={AlertCircleIcon}
          size={24}
          className="text-destructive"
        />
      </div>
      <div className="space-y-2">
        <h1 className="text-2xl font-bold tracking-tight">
          Something went wrong
        </h1>
        <p className="text-sm text-muted-foreground">
          An unexpected error occurred. Try refreshing or go back.
        </p>
        {import.meta.env.DEV && error instanceof Error && (
          <pre className="mt-4 max-w-lg overflow-x-auto border bg-muted/50 p-3 text-left font-mono text-xs text-muted-foreground">
            {error.message}
          </pre>
        )}
      </div>
      <div className="flex items-center gap-3">
        <Button
          variant="outline"
          onClick={() => {
            reset()
            router.invalidate()
          }}
        >
          <HugeiconsIcon icon={RotateClockwiseIcon} size={16} />
          Try again
        </Button>
        <Button
          variant="outline"
          onClick={() => window.history.back()}
        >
          <HugeiconsIcon icon={ArrowLeft02Icon} size={16} />
          Go back
        </Button>
      </div>
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
  notFoundComponent: NotFound,
  errorComponent: RootError,
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

  useSessionMonitor()

  useEffect(() => {
    useSetupStore.getState().setInstanceContext(activeInstanceId)
    useAuthStore.getState().setInstanceContext(activeInstanceId)
  }, [activeInstanceId])

  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <QueryClientProvider client={queryClient}>
        <RouteTransitionBar />
        {showAppChrome ? (
          <SidebarProvider>
            <AppSidebar />
            <SidebarInset>
              <header className="sticky top-0 z-30 flex h-12 shrink-0 items-center gap-2 border-b bg-background px-4">
                <SidebarTrigger className="-ml-1" />
                <Separator
                  orientation="vertical"
                  className="mr-2 h-4! self-auto!"
                />
                <Link to="/" className="flex items-center gap-2 pr-1">
                  <img
                    src="/logo.svg"
                    alt="Oore CI logo"
                    className="size-5"
                  />
                  <span className="hidden text-sm font-semibold tracking-tight sm:inline">
                    Oore CI
                  </span>
                </Link>
                <Separator
                  orientation="vertical"
                  className="mr-2 h-4! self-auto!"
                />
                <PageBreadcrumb />
                <div className="ml-auto">
                  <button
                    type="button"
                    onClick={() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', metaKey: true }))}
                    className="inline-flex h-8 items-center gap-2 rounded-sm border bg-muted/50 px-2.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
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
        {showAppChrome ? (
          <Suspense fallback={null}>
            <CommandPalette />
          </Suspense>
        ) : null}
        {import.meta.env.VITE_DEMO_MODE === 'true' && (
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
              Learn more
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
