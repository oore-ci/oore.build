import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { RuntimeReleaseStatus, RuntimeUpdateStatus } from '@/lib/types'
import { getBackendUpdateStatus, startBackendUpdate } from '@/lib/api'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

interface BackendRelease {
  version?: string
  channel?: string | null
  github_repo?: string | null
}

async function localUpdateRequest<T>(
  token: string,
  method: 'GET' | 'POST',
  search?: URLSearchParams,
): Promise<T> {
  const response = await fetch(
    `/__oore_web_update${search ? `?${search}` : ''}`,
    {
      method,
      headers: { Authorization: `Bearer ${token}` },
      cache: 'no-store',
    },
  )
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as {
      error?: string
    } | null
    throw new Error(body?.error || `Update request failed (${response.status})`)
  }
  return (await response.json()) as T
}

export function useRuntimeUpdates(backend: BackendRelease) {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const token = useAuthStore((state) => state.token)
  const isOwner = useAuthStore((state) => state.user?.role === 'owner')
  const instanceKey = instance?.id ?? '__none__'

  const frontendRelease = useQuery({
    queryKey: [instanceKey, 'runtime-update', 'frontend-release'],
    queryFn: () => localUpdateRequest<RuntimeReleaseStatus>(token!, 'GET'),
    enabled: !!token && isOwner,
    staleTime: 60_000,
    refetchInterval: (query) =>
      query.state.data?.phase === 'updating' ||
      query.state.data?.phase === 'restarting'
        ? 2_000
        : false,
  })

  const backendRelease = useQuery({
    queryKey: [
      instanceKey,
      'runtime-update',
      'backend-release',
      backend.version,
      backend.channel,
      backend.github_repo,
    ],
    queryFn: () =>
      localUpdateRequest<RuntimeReleaseStatus>(
        token!,
        'GET',
        new URLSearchParams({
          current: backend.version!,
          channel: backend.channel!,
          repo: backend.github_repo!,
        }),
      ),
    enabled:
      !!token &&
      isOwner &&
      !!backend.version &&
      !!backend.channel &&
      !!backend.github_repo,
    staleTime: 60_000,
  })

  const backendUpdate = useQuery({
    queryKey: [instanceKey, 'runtime-update', 'backend-state'],
    queryFn: () => getBackendUpdateStatus(baseUrl!, token!),
    enabled: !!baseUrl && !!token && isOwner,
    refetchInterval: (query) =>
      query.state.data?.phase === 'updating' ||
      query.state.data?.phase === 'restarting'
        ? 2_000
        : false,
  })

  const startFrontendUpdate = useMutation({
    mutationFn: () => localUpdateRequest<RuntimeUpdateStatus>(token!, 'POST'),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instanceKey, 'runtime-update'],
      })
    },
  })

  const startBackendUpdateMutation = useMutation({
    mutationFn: () => startBackendUpdate(baseUrl!, token!),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instanceKey, 'runtime-update'],
      })
    },
  })

  return {
    frontendRelease,
    backendRelease,
    backendUpdate,
    startFrontendUpdate,
    startBackendUpdate: startBackendUpdateMutation,
  }
}
