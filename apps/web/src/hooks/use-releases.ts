import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import type {
  ListReleasesResponse,
  Release,
  ReleaseAction,
  ReleaseActionRequest,
} from '@oore/client/models'
import {
  approveRelease,
  completeRelease,
  haltRelease,
  pauseRelease,
  raiseRelease,
  refreshStoreVitals,
  resumeRelease,
} from '@oore/client/operations'
import {
  getReleaseOptions,
  getReleaseQueryKey,
  getStoreVitalsOptions,
  getStoreVitalsQueryKey,
  listReleaseEventsOptions,
  listReleaseEventsQueryKey,
  listReleasesOptions,
  listReleasesQueryKey,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'
import { LIVE_FALLBACK_POLL_MS } from '@/lib/live-events'
import { isLiveRelease } from '@/lib/releases'
import { useLiveEventsStore } from '@/stores/live-events-store'

// Matches the build poll so rollout changes land on the same cadence.
export const RELEASE_POLL_INTERVAL_MS = 3_000

function useReleasePollInterval() {
  const liveEvents = useLiveEventsStore((s) => s.connected)
  return liveEvents ? LIVE_FALLBACK_POLL_MS : RELEASE_POLL_INTERVAL_MS
}

export function useReleases<TData = ListReleasesResponse>(
  params?: {
    project_id?: string
    build_id?: string
    state?: string
    limit?: number
    offset?: number
  },
  options?: {
    enabled?: boolean
    select?: (data: ListReleasesResponse) => TData
  },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const pollInterval = useReleasePollInterval()
  const query = scopeOoreQueryOptions(
    instanceId,
    listReleasesOptions({ client, query: params }),
  )

  return useQuery<ListReleasesResponse, Error, TData>({
    ...query,
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    staleTime: 2_000,
    refetchInterval: (query) =>
      query.state.data?.releases.some(isLiveRelease) ? pollInterval : false,
    placeholderData: keepPreviousData,
    select: options?.select,
  })
}

/** Store crash, ANR, hang and launch numbers the daemon caches per app. */
export function useStoreVitals() {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getStoreVitalsOptions({ client }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token,
    // The daemon refreshes stores every few hours; minutes are fresh enough.
    staleTime: 5 * 60_000,
    refetchInterval: 15 * 60_000,
  })
}

/** Read the stores now instead of waiting for the daemon's next pass. */
export function useRefreshStoreVitals() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: () => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return refreshStoreVitals({ client })
    },
    onSuccess: (vitals) => {
      queryClient.setQueryData(
        scopeOoreQueryKey(instanceId, getStoreVitalsQueryKey({ client })),
        vitals,
      )
    },
  })
}

export function useRelease(releaseId: string, options?: { enabled?: boolean }) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const pollInterval = useReleasePollInterval()
  const query = scopeOoreQueryOptions(
    instanceId,
    getReleaseOptions({ client, path: { release_id: releaseId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!releaseId && (options?.enabled ?? true),
    staleTime: 2_000,
    refetchInterval: (query) =>
      query.state.data && isLiveRelease(query.state.data.release)
        ? pollInterval
        : false,
  })
}

export function useReleaseEvents(
  params: { project_id?: string; limit?: number },
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const pollInterval = useReleasePollInterval()
  const query = scopeOoreQueryOptions(
    instanceId,
    listReleaseEventsOptions({ client, query: params }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    staleTime: 5_000,
    refetchInterval: Math.max(pollInterval, RELEASE_POLL_INTERVAL_MS * 5),
  })
}

const RELEASE_ACTION_OPERATIONS = {
  approve: approveRelease,
  complete: completeRelease,
  halt: haltRelease,
  pause: pauseRelease,
  raise: raiseRelease,
  resume: resumeRelease,
} satisfies Record<ReleaseAction, typeof haltRelease>

export function useReleaseAction() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: ({
      action,
      body,
      releaseId,
    }: {
      action: ReleaseAction
      body: ReleaseActionRequest
      releaseId: string
    }): Promise<Release> => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return RELEASE_ACTION_OPERATIONS[action]({
        body,
        client,
        path: { release_id: releaseId },
      })
    },
    onSuccess: (_release, { releaseId }) => {
      for (const queryKey of [
        listReleasesQueryKey({ client }),
        listReleaseEventsQueryKey({ client }),
        getReleaseQueryKey({ client, path: { release_id: releaseId } }),
      ]) {
        void queryClient.invalidateQueries({
          queryKey: scopeOoreQueryKey(instanceId, queryKey),
        })
      }
    },
  })
}
