import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { UpdateRetentionPolicyRequest } from '@/lib/types'
import {
  getRetentionLastCleanup,
  getRetentionPolicy,
  updateRetentionPolicy,
} from '@/lib/api'
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

export function useRetentionPolicy() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'retention-policy'],
    queryFn: ({ signal }) => getRetentionPolicy(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useUpdateRetentionPolicy() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'retention-last-cleanup'],
    queryFn: ({ signal }) =>
      getRetentionLastCleanup(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}
