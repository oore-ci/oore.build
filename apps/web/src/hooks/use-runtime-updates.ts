import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as z from 'zod'

import type { QueryClient } from '@tanstack/react-query'
import type { RuntimeUpdateStatus } from '@oore/client/models'
import type { OoreClient } from '@oore/client'
import {
  getRuntimeUpdateStatus,
  readyz,
  startRuntimeUpdate as startBackendUpdate,
} from '@oore/client/operations'
import {
  getRuntimeUpdateStatusQueryKey,
  healthzOptions,
} from '@oore/client/react-query'
import { toast } from '@/lib/toast'
import { useAuthStore } from '@/stores/auth-store'
import { useRuntimeUpdateStore } from '@/stores/runtime-update-store'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'
import {
  backendRunsRelease,
  normalizeVersion,
  planCoordinatedUpdate,
  releaseIsOffered,
} from '@/lib/runtime-update-plan'

interface BackendRelease {
  version?: string
  channel?: string | null
  github_repo?: string | null
}

/** One published release between the installed and the latest version. */
export interface RuntimeReleaseEntry {
  version: string
  tag: string
  published_at: string
  release_name: string
  release_notes: string
  release_url: string
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
  /** Compare link from the installed tag when it was published, else the latest release's changelog. */
  changelog_url: string
  /** Every release after `version` up to `latest_version`, newest first. Missing when oore-web could not read the release history. */
  releases?: Array<RuntimeReleaseEntry>
  /** Why oore-web cannot update itself from the UI, when it cannot. */
  managed_service_reason?: string | null
  /** Update log on the web node, kept after a failed update. */
  log_path?: string | null
}

const HEALTH_REFRESH_INTERVAL = 60_000
const RELEASE_REFRESH_INTERVAL = 5 * 60_000
const UPDATE_POLL_INTERVAL = 3_000
// The backend supervisor drains running builds before it restarts oored.
const BACKEND_UPDATE_TIMEOUT = 30 * 60_000
const FRONTEND_UPDATE_TIMEOUT = 10 * 60_000

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
const readinessSchema = z.object({
  ok: z.boolean(),
  version: z.string().optional(),
  package_version: z.string().optional(),
})
const updateErrorSchema = z.object({ error: z.string().optional() })

function isBusy(phase: string | undefined) {
  return phase === 'updating' || phase === 'restarting'
}

function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** Polls until `check` returns true. Transient failures are retried; thrown errors from `check` end the wait. */
async function waitUntil(
  check: () => Promise<boolean>,
  timeoutMs: number,
  timeoutMessage: string,
) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (await check()) return
    await delay(UPDATE_POLL_INTERVAL)
  }
  throw new Error(timeoutMessage)
}

async function waitForBackendRelease(client: OoreClient, target: string) {
  await waitUntil(
    async () => {
      const status = await getRuntimeUpdateStatus({ client }).catch(() => null)
      if (status?.phase === 'failed') {
        throw new Error(status.error || 'The backend update failed')
      }
      if (status?.phase !== 'idle') return false
      const ready = await readyz({ client })
        .then((data) => readinessSchema.parse(data))
        .catch(() => null)
      if (!ready?.ok) return false
      if (!backendRunsRelease(ready, target)) {
        const running = normalizeVersion(ready.version) || 'an unknown version'
        throw new Error(
          `The backend finished on ${running} instead of ${target}. The frontend was not updated.`,
        )
      }
      return true
    },
    BACKEND_UPDATE_TIMEOUT,
    `The backend did not report ${target} as ready in time. Check Settings for its update status.`,
  )
}

async function waitForFrontendRelease(token: string, target: string) {
  await waitUntil(
    async () => {
      const status = await localUpdateRequest<RuntimeReleaseStatus>(
        token,
        'GET',
      ).catch(() => null)
      if (status?.phase === 'failed') {
        throw new Error(status.error || 'The frontend update failed')
      }
      if (status?.phase !== 'idle') return false
      const installed = normalizeVersion(status.version)
      if (installed !== target) {
        throw new Error(
          `The frontend finished on ${installed} instead of ${target}.`,
        )
      }
      return true
    },
    FRONTEND_UPDATE_TIMEOUT,
    `The frontend did not restart on ${target} in time.`,
  )
}

async function refetchAfterPhaseSettles(
  queryClient: QueryClient,
  previousPhase: string | undefined,
  nextPhase: string | undefined,
  queryKey: readonly unknown[],
) {
  // Refresh the reported version as soon as an update settles, so the release
  // check is rekeyed before the badge can offer the update it just applied.
  if (isBusy(previousPhase) && !isBusy(nextPhase)) {
    await queryClient.refetchQueries({ queryKey, exact: true })
  }
}

// TODO(oore-client-js): switch to the generated operation once the client is
// regenerated with `GET /v1/system/update/preflight`.
const runtimeUpdatePreflightSchema = z.object({
  ready: z.boolean(),
  checked_at: z.number(),
  checks: z.array(
    z.object({
      id: z.string(),
      label: z.string(),
      status: z.enum(['pass', 'warn', 'fail']),
      detail: z.string().nullish(),
      fix: z.string().nullish(),
    }),
  ),
})

export type RuntimeUpdatePreflight = z.infer<
  typeof runtimeUpdatePreflightSchema
>
export type RuntimeUpdateCheck = RuntimeUpdatePreflight['checks'][number]

function backendFailedCheckReason(
  preflight: RuntimeUpdatePreflight | undefined,
) {
  const failed = preflight?.checks.find((check) => check.status === 'fail')
  return failed
    ? `The backend update check "${failed.label}" failed. The Backend card below says what to run.`
    : undefined
}

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

  const frontendHealthKey = ['runtime-health', 'oore-web']
  const frontendHealth = useQuery({
    queryKey: frontendHealthKey,
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
  // The frontend follows the backend's channel, so a backend that moved to
  // another channel brings the frontend onto the same release stream.
  const frontendSearch = backend.channel
    ? new URLSearchParams({ channel: backend.channel })
    : undefined

  const frontendReleaseKey = [
    instanceKey,
    'runtime-update',
    'frontend-release',
    backend.channel ?? null,
  ]
  const frontendRelease = useQuery({
    queryKey: frontendReleaseKey,
    queryFn: async ({ signal }) => {
      const previous =
        queryClient.getQueryData<RuntimeReleaseStatus>(frontendReleaseKey)
      const status = await localUpdateRequest<RuntimeReleaseStatus>(
        token!,
        'GET',
        frontendSearch,
        signal,
      )
      await refetchAfterPhaseSettles(
        queryClient,
        previous?.phase,
        status.phase,
        frontendHealthKey,
      )
      return status
    },
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

  const backendUpdateKey = scopeOoreQueryKey(
    instanceId,
    getRuntimeUpdateStatusQueryKey({ client }),
  )
  const backendUpdate = useQuery({
    queryKey: backendUpdateKey,
    queryFn: async ({ signal }) => {
      const previous =
        queryClient.getQueryData<RuntimeUpdateStatus>(backendUpdateKey)
      const status = await getRuntimeUpdateStatus({ client, signal })
      await refetchAfterPhaseSettles(
        queryClient,
        previous?.phase,
        status.phase,
        backendHealthOptions.queryKey,
      )
      return status
    },
    enabled: !!baseUrl && !!token && isOwner,
    refetchInterval: (query) =>
      query.state.data?.phase === 'updating' ||
      query.state.data?.phase === 'restarting'
        ? 2_000
        : false,
  })

  const backendPreflight = useQuery({
    queryKey: scopeOoreQueryKey(instanceId, [
      'runtime-update',
      'backend-preflight',
    ]),
    queryFn: async ({ signal }) =>
      runtimeUpdatePreflightSchema.parse(
        await client.get({
          responseStyle: 'data',
          security: [{ scheme: 'bearer', type: 'http' }],
          url: '/v1/system/update/preflight',
          signal,
        }),
      ),
    enabled: !!baseUrl && !!token && isOwner,
    retry: false,
    staleTime: 30_000,
  })

  const startFrontendUpdate = useMutation({
    mutationFn: () =>
      localUpdateRequest<RuntimeUpdateStatus>(token!, 'POST', frontendSearch),
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
      void queryClient.invalidateQueries({ queryKey: backendUpdateKey })
    },
    // A refused start usually means a check changed; show the current reason.
    onError: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(instanceId, [
          'runtime-update',
          'backend-preflight',
        ]),
      })
    },
  })

  const frontendOffered = releaseIsOffered(
    frontendRelease.data,
    frontendHealth.data,
  )
  const backendOffered = releaseIsOffered(backendRelease.data, backend)
  const coordinatedPlan = planCoordinatedUpdate({
    frontend: frontendRelease.data,
    backend: backendRelease.data,
    frontendOffered,
    backendOffered,
    backendManaged:
      backendPreflight.data?.ready ??
      backendUpdate.data?.managed_service === true,
    backendUnavailableReason: backendFailedCheckReason(backendPreflight.data),
  })

  const coordinated = useRuntimeUpdateStore()
  const updateOore = useMutation({
    mutationKey: [instanceKey, 'runtime-update', 'coordinated'],
    mutationFn: async () => {
      if (coordinatedPlan.kind !== 'ready') {
        throw new Error(coordinatedPlan.reason)
      }
      const { target } = coordinatedPlan
      // Backend first: the running frontend's client must never be newer
      // than the API it talks to.
      if (coordinatedPlan.backend) {
        coordinated.setStep('backend', target)
        await startBackendUpdate({ client })
        await queryClient.invalidateQueries({ queryKey: backendUpdateKey })
        await waitForBackendRelease(client, target)
        await queryClient.refetchQueries({
          queryKey: backendHealthOptions.queryKey,
          exact: true,
        })
      }
      if (coordinatedPlan.frontend) {
        coordinated.setStep('frontend', target)
        await localUpdateRequest<RuntimeUpdateStatus>(
          token!,
          'POST',
          frontendSearch,
        )
        await waitForFrontendRelease(token!, target)
        // The page still runs the previous bundle; load the one just installed.
        coordinated.setStep('reloading')
        window.location.reload()
        return
      }
      coordinated.setStep('idle', null)
      toast.success(`Oore is now running ${target}.`)
    },
    onError: (error) => coordinated.fail(error.message),
    onSettled: () => {
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
    backendPreflight,
    startFrontendUpdate,
    startBackendUpdate: startBackendUpdateMutation,
    frontendOffered,
    backendOffered,
    coordinatedPlan,
    coordinatedStep: coordinated.step,
    coordinatedTarget: coordinated.target,
    coordinatedError: coordinated.error,
    updateOore,
  }
}
