import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { RuntimeReleaseStatus, RuntimeUpdateStatus } from '@/lib/types'
import { getBackendUpdateStatus, startBackendUpdate } from '@/lib/api'
import { useAuthStore } from '@/stores/auth-store'
import { useApiContext } from '@/hooks/use-api-context'

interface BackendRelease {
  version?: string
  channel?: string | null
  github_repo?: string | null
}

const HEALTH_REFRESH_INTERVAL = 60_000
const RELEASE_REFRESH_INTERVAL = 5 * 60_000

export interface RuntimeHealth extends BackendRelease {
  ok?: boolean
  package_version?: string
}

async function fetchRuntimeHealth(
  path: string,
  signal?: AbortSignal,
): Promise<RuntimeHealth> {
  const response = await fetch(path, { cache: 'no-store', signal })
  if (!response.ok) {
    throw new Error(`Health check failed (${response.status})`)
  }
  return (await response.json()) as RuntimeHealth
}

async function localUpdateRequest<T>(
  token: string,
  method: 'GET' | 'POST',
  search?: URLSearchParams,
  signal?: AbortSignal,
): Promise<T> {
  const response = await fetch(
    `/__oore_web_update${search ? `?${search}` : ''}`,
    {
      method,
      headers: { Authorization: `Bearer ${token}` },
      cache: 'no-store',
      signal,
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

export function useRuntimeUpdates() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()
  const isOwner = useAuthStore((state) => state.user?.role === 'owner')
  const instanceKey = instance?.id ?? '__none__'

  const frontendHealth = useQuery({
    queryKey: ['runtime-health', 'oore-web'],
    queryFn: ({ signal }) => fetchRuntimeHealth('/__oore_web_healthz', signal),
    retry: false,
    staleTime: 30_000,
    refetchInterval: HEALTH_REFRESH_INTERVAL,
  })

  const backendHealth = useQuery({
    queryKey: [instanceKey, 'runtime-health', 'oored'],
    queryFn: ({ signal }) => {
      if (!baseUrl) throw new Error('No active instance URL')
      return fetchRuntimeHealth(new URL('/healthz', baseUrl).toString(), signal)
    },
    enabled: !!baseUrl,
    retry: false,
    staleTime: 30_000,
    refetchInterval: HEALTH_REFRESH_INTERVAL,
  })

  const backend = backendHealth.data ?? {}

  const frontendRelease = useQuery({
    queryKey: [instanceKey, 'runtime-update', 'frontend-release'],
    queryFn: ({ signal }) =>
      localUpdateRequest<RuntimeReleaseStatus>(
        token!,
        'GET',
        undefined,
        signal,
      ),
    enabled: !!token && isOwner,
    staleTime: 60_000,
    refetchInterval: (query) =>
      query.state.data?.phase === 'updating' ||
      query.state.data?.phase === 'restarting'
        ? 2_000
        : RELEASE_REFRESH_INTERVAL,
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
    queryFn: ({ signal }) =>
      localUpdateRequest<RuntimeReleaseStatus>(
        token!,
        'GET',
        new URLSearchParams({
          current: backend.version!,
          channel: backend.channel!,
          repo: backend.github_repo!,
        }),
        signal,
      ),
    enabled:
      !!token &&
      isOwner &&
      !!backend.version &&
      !!backend.channel &&
      !!backend.github_repo,
    staleTime: 60_000,
    refetchInterval: RELEASE_REFRESH_INTERVAL,
  })

  const backendUpdate = useQuery({
    queryKey: [instanceKey, 'runtime-update', 'backend-state'],
    queryFn: ({ signal }) =>
      getBackendUpdateStatus(baseUrl!, token!, { signal }),
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
    frontendHealth,
    backendHealth,
    frontendRelease,
    backendRelease,
    backendUpdate,
    startFrontendUpdate,
    startBackendUpdate: startBackendUpdateMutation,
  }
}
