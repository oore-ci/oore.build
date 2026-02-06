import { lazy, Suspense } from 'react'
import { Outlet, createRootRoute, useMatches } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'

import Header from '../components/Header'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5000,
      retry: 1,
    },
  },
})

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
  component: RootLayout,
})

function RootLayout() {
  const matches = useMatches()
  const isSetupRoute = matches.some((m) => m.fullPath.startsWith('/setup'))

  return (
    <QueryClientProvider client={queryClient}>
      {isSetupRoute ? null : <Header />}
      <Outlet />
      <Suspense>
        <DevTools />
      </Suspense>
    </QueryClientProvider>
  )
}
