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

const HEALTH_REFRESH_INTERVAL = 60_000
const RELEASE_REFRESH_INTERVAL = 5 * 60_000

export interface RuntimeHealth extends BackendRelease {
  ok?: boolean
  package_version?: string
}

async function fetchRuntimeHealth(path: string): Promise<RuntimeHealth> {
  const response = await fetch(path, { cache: 'no-store' })
  if (!response.ok) {
    throw new Error(`Health check failed (${response.status})`)
  }
  return (await response.json()) as RuntimeHealth
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

export function useRuntimeUpdates() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const token = useAuthStore((state) => state.token)
  const isOwner = useAuthStore((state) => state.user?.role === 'owner')
  const instanceKey = instance?.id ?? '__none__'

  const frontendHealth = useQuery({
    queryKey: ['runtime-health', 'oore-web'],
    queryFn: () => fetchRuntimeHealth('/__oore_web_healthz'),
    retry: false,
    staleTime: 30_000,
    refetchInterval: HEALTH_REFRESH_INTERVAL,
  })

  const backendHealth = useQuery({
    queryKey: [instanceKey, 'runtime-health', 'oored'],
    queryFn: () => {
      if (!baseUrl) throw new Error('No active instance URL')
      return fetchRuntimeHealth(new URL('/healthz', baseUrl).toString())
    },
    enabled: !!baseUrl,
    retry: false,
    staleTime: 30_000,
    refetchInterval: HEALTH_REFRESH_INTERVAL,
  })

  const backend = backendHealth.data ?? {}

  const frontendRelease = useQuery({
    queryKey: [instanceKey, 'runtime-update', 'frontend-release'],
    queryFn: () => localUpdateRequest<RuntimeReleaseStatus>(token!, 'GET'),
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
    refetchInterval: RELEASE_REFRESH_INTERVAL,
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
    frontendHealth,
    backendHealth,
    frontendRelease,
    backendRelease,
    backendUpdate,
    startFrontendUpdate,
    startBackendUpdate: startBackendUpdateMutation,
  }
}
