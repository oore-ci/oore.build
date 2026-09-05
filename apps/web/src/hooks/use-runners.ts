import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { updateRunner } from '@oore/client/operations'
import {
  listRunnersOptions,
  listRunnersQueryKey,
} from '@oore/client/react-query'
import type {
  ListRunnersData,
  ListRunnersResponse,
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
