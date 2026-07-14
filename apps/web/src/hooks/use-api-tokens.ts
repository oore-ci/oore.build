import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { CreateApiTokenRequest } from '@/lib/types'
import { createApiToken, listApiTokens, revokeApiToken } from '@/lib/api'
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

export function useApiTokens() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'api-tokens'],
    queryFn: () => listApiTokens(baseUrl!, token!),
    enabled: !!baseUrl && !!token,
  })
}

export function useCreateApiToken() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
