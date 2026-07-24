import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { UpdateRetentionPolicyRequest } from '@/lib/types'
import {
  getRetentionLastCleanup,
  getRetentionPolicy,
  updateRetentionPolicy,
} from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

export function useRetentionPolicy() {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'retention-policy'],
    queryFn: ({ signal }) => getRetentionPolicy(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useUpdateRetentionPolicy() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (data: UpdateRetentionPolicyRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateRetentionPolicy(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'retention-policy'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'retention-last-cleanup'],
      })
    },
  })
}

export function useRetentionLastCleanup() {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'retention-last-cleanup'],
    queryFn: ({ signal }) =>
      getRetentionLastCleanup(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}
