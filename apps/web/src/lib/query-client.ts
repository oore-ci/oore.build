import { MutationCache, QueryCache, QueryClient } from '@tanstack/react-query'

import { isUnauthorizedError, reportApiError } from '@/lib/session-events'

export const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError: reportApiError }),
  mutationCache: new MutationCache({ onError: reportApiError }),
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      // A rejected session will not recover by retrying.
      retry: (failureCount, error) =>
        failureCount < 1 && !isUnauthorizedError(error),
    },
  },
})
