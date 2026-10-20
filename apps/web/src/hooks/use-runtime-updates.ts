import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as z from 'zod'

import type { RuntimeUpdateStatus } from '@oore/client/models'
import { startRuntimeUpdate as startBackendUpdate } from '@oore/client/operations'
import {
  getRuntimeUpdateStatusOptions,
  getRuntimeUpdateStatusQueryKey,
  healthzOptions,
} from '@oore/client/react-query'
import { useAuthStore } from '@/stores/auth-store'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

interface BackendRelease {
  version?: string
  channel?: string | null
  github_repo?: string | null
}

export interface RuntimeReleaseStatus extends RuntimeUpdateStatus {
  version: string
  latest_version: string
  channel: string
  github_repo: string
  update_available: boolean
  release_name: string
  release_notes: string
  release_url: string
  changelog_url: string
  /** Why oore-web cannot update itself from the UI, when it cannot. */
  managed_service_reason?: string | null
  /** Update log on the web node, kept after a failed update. */
  log_path?: string | null
}

const HEALTH_REFRESH_INTERVAL = 60_000
const RELEASE_REFRESH_INTERVAL = 5 * 60_000

export interface RuntimeHealth extends BackendRelease {
  ok?: boolean
  package_version?: string
}

const runtimeHealthSchema = z.object({
  ok: z.boolean().optional(),
  package_version: z.string().optional(),
  version: z.string().optional(),
  channel: z.string().nullable().optional(),
  github_repo: z.string().nullable().optional(),
})
const updateErrorSchema = z.object({ error: z.string().optional() })

async function fetchRuntimeHealth(
  path: string,
  signal?: AbortSignal,
): Promise<RuntimeHealth> {
  const response = await fetch(path, {
    cache: 'no-store',
    credentials: 'include',
    signal,
  })
  if (!response.ok) {
    throw new Error(`Health check failed (${response.status})`)
  }
  return runtimeHealthSchema.parse(await response.json())
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
    const body = updateErrorSchema
      .nullable()
      .parse(await response.json().catch(() => null))
    throw new Error(body?.error || `Update request failed (${response.status})`)
  }
  // SAFETY: Each caller binds T to the fixed local update endpoint and method used for this request.
  return (await response.json()) as T
}

export function useRuntimeUpdates() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instance, instanceId, token } = useApiContext()
  const isOwner = useAuthStore((state) => state.user?.role === 'owner')
  const instanceKey = instance?.id ?? '__none__'

  const frontendHealth = useQuery({
    queryKey: ['runtime-health', 'oore-web'],
    queryFn: ({ signal }) => fetchRuntimeHealth('/__oore_web_healthz', signal),
    retry: false,
    staleTime: 30_000,
    refetchInterval: HEALTH_REFRESH_INTERVAL,
  })

  const backendHealthOptions = scopeOoreQueryOptions(
    instanceId,
    healthzOptions({ client }),
  )
  const backendHealth = useQuery({
    ...backendHealthOptions,
    select: (data) => runtimeHealthSchema.parse(data),
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

  const backendUpdateQuery = scopeOoreQueryOptions(
    instanceId,
    getRuntimeUpdateStatusOptions({ client }),
  )
  const backendUpdate = useQuery({
    ...backendUpdateQuery,
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
    mutationFn: () => startBackendUpdate({ client }),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instanceKey, 'runtime-update'],
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getRuntimeUpdateStatusQueryKey({ client }),
        ),
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
