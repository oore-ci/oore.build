import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { CreateApiTokenRequest } from '@/lib/types'
import { createApiToken, listApiTokens, revokeApiToken } from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

export function useApiTokens() {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'api-tokens'],
    queryFn: ({ signal }) => listApiTokens(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useCreateApiToken() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (data: CreateApiTokenRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return createApiToken(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'api-tokens'],
      })
    },
  })
}

export function useRevokeApiToken() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (tokenId: string) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return revokeApiToken(baseUrl, token, tokenId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'api-tokens'],
      })
    },
  })
}
