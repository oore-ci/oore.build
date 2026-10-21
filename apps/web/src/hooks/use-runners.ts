import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as z from 'zod'

import { updateRunner } from '@oore/client/operations'
import {
  listRunnersOptions,
  listRunnersQueryKey,
} from '@oore/client/react-query'
import type {
  ListRunnersData,
  ListRunnersResponse,
  Runner,
  UpdateRunnerRequest,
} from '@oore/client/models'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

type ListRunnersParams = NonNullable<ListRunnersData['query']>

export function useRunners<TData = ListRunnersResponse>(
  params?: ListRunnersParams,
  options?: {
    enabled?: boolean
    select?: (data: ListRunnersResponse) => TData
  },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listRunnersOptions({ client, query: params }),
  )

  return useQuery<ListRunnersResponse, Error, TData>({
    ...query,
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    refetchInterval: 15_000,
    select: options?.select,
  })
}

export function useUpdateRunner() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: ({
      runnerId,
      data,
    }: {
      runnerId: string
      data: UpdateRunnerRequest
    }) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateRunner({
        body: data,
        client,
        path: { runner_id: runnerId },
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listRunnersQueryKey({ client }),
        ),
      })
    },
  })
}

const runnerCacheStatsSchema = z.object({
  total_bytes: z.number(),
  max_bytes: z.number(),
  measured_at: z.number(),
})

export type RunnerCacheStats = z.infer<typeof runnerCacheStatsSchema>

/** Dependency cache sizes a runner reports in `capabilities.caches`. */
export function runnerCacheStats(runner: Runner): RunnerCacheStats | undefined {
  return runnerCacheStatsSchema.safeParse(runner.capabilities.caches).data
}

/** Set while a purge waits for the runner; not yet in the generated client. */
export function runnerCachePurgeRequestedAt(
  runner: Runner,
): number | undefined {
  return z.object({ cache_purge_requested_at: z.number() }).safeParse(runner)
    .data?.cache_purge_requested_at
}

export function usePurgeRunnerCaches() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: async (runnerId: string) => {
      if (!baseUrl || !token) {
        throw new Error('Not authenticated')
      }
      await client.post({
        responseStyle: 'data',
        security: [{ scheme: 'bearer', type: 'http' }],
        url: `/v1/runners/${encodeURIComponent(runnerId)}/caches/purge`,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listRunnersQueryKey({ client }),
        ),
      })
    },
  })
}
