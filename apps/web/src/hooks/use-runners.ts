import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { ListRunnersResponse, UpdateRunnerRequest } from '@/lib/types'
import { listRunners, updateRunner } from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

export function useRunners<TData = ListRunnersResponse>(options?: {
  select?: (data: ListRunnersResponse) => TData
}) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery<ListRunnersResponse, Error, TData>({
    queryKey: [instance?.id ?? '__none__', 'runners'],
    queryFn: ({ signal }) => listRunners(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
    refetchInterval: 15_000,
    select: options?.select,
  })
}

export function useUpdateRunner() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
      return updateRunner(baseUrl, token, runnerId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'runners'],
      })
    },
  })
}
