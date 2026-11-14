import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { NotificationPreferences, Project } from '@oore/client/models'
import {
  addProjectFavorite,
  removeProjectFavorite,
  sendWeeklyDigest,
  updateNotificationPreferences,
} from '@oore/client/operations'
import {
  getInboxOptions,
  getNotificationPreferencesOptions,
  getNotificationPreferencesQueryKey,
  getProjectQueryKey,
  getWeeklyDigestOptions,
  listProjectsQueryKey,
} from '@oore/client/react-query'

import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

/** The inbox refreshes this often; opening the bell refetches it too. */
const INBOX_POLL_MS = 60_000

export function useNotificationPreferences() {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getNotificationPreferencesOptions({ client }),
    ),
    enabled: !!baseUrl && !!token,
    staleTime: 60_000,
  })
}

export function useUpdateNotificationPreferences() {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (preferences: NotificationPreferences) =>
      updateNotificationPreferences({
        body: preferences,
        client,
        throwOnError: true,
      }),
    onSuccess: (response) => {
      queryClient.setQueryData(
        scopeOoreQueryKey(
          instanceId,
          getNotificationPreferencesQueryKey({ client }),
        ),
        response,
      )
    },
  })
}

export function useInbox(options?: { enabled?: boolean }) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(instanceId, getInboxOptions({ client })),
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    refetchInterval: INBOX_POLL_MS,
    staleTime: 15_000,
  })
}

export function useWeeklyDigest(options?: { enabled?: boolean }) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(instanceId, getWeeklyDigestOptions({ client })),
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    staleTime: 5 * 60_000,
  })
}

export function useSendWeeklyDigest() {
  const { client } = useApiContext()
  return useMutation({
    mutationFn: () => sendWeeklyDigest({ client, throwOnError: true }),
  })
}

/** Pin or unpin an app for yourself. */
export function useToggleFavorite() {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (project: Pick<Project, 'id' | 'is_favorite'>) => {
      const options = { client, path: { project_id: project.id } }
      return project.is_favorite
        ? removeProjectFavorite({ ...options, throwOnError: true })
        : addProjectFavorite({ ...options, throwOnError: true })
    },
    onSettled: (_data, _error, project) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectsQueryKey({ client }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getProjectQueryKey({ client, path: { project_id: project.id } }),
        ),
      })
    },
  })
}
