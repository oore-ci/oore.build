import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { ListRunnersResponse, UpdateRunnerRequest } from '@/lib/types'
import { listRunners, updateRunner } from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'

function useAuthToken(): string | null {
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  if (!token || expiresAt == null) return null
  if (expiresAt <= Math.floor(Date.now() / 1000)) return null
  return token
}

function useBaseUrl(): string | null {
  const instance = useActiveInstance()
  return resolveInstanceApiBaseUrl(instance)
}

export function useRunners<TData = ListRunnersResponse>(options?: {
  select?: (data: ListRunnersResponse) => TData
}) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
