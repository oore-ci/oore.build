import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import type { UpdateRunnerRequest } from '@/lib/types'
import { listRunners, updateRunner } from '@/lib/api'
import {
  useAuthToken,
  useBaseUrl,
  useInstanceQueryPrefix,
} from '@/hooks/query-context'

export function useRunners() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'runners'],
    queryFn: () => listRunners(baseUrl()!, token()!),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useUpdateRunner() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async ({
      runnerId,
      data,
    }: {
      runnerId: string
      data: UpdateRunnerRequest
    }) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return updateRunner(baseUrl()!, token()!, runnerId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'runners'] })
    },
  }))
}
