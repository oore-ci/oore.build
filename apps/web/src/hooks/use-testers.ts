import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  BuildPlatform,
  SendBuildToGroupRequest,
  TesterGroupRequest,
} from '@oore/client/models'
import {
  createDeviceRegistration,
  createLatestInstallLink,
  createTesterGroup,
  deleteLatestInstallLink,
  deleteTesterGroup,
  keepBuild,
  sendBuildToGroup,
  sendTestWebPush,
  subscribeWebPush,
  unkeepBuild,
  unsubscribeWebPush,
  updateTesterGroup,
} from '@oore/client/operations'
import {
  getBuildQueryKey,
  getDeviceRegistrationOptions,
  getWebPushConfigOptions,
  getWebPushConfigQueryKey,
  listBuildInstallsOptions,
  listBuildSharesOptions,
  listBuildSharesQueryKey,
  listLatestInstallLinksOptions,
  listLatestInstallLinksQueryKey,
  listRecentFeedbackOptions,
  listTesterGroupsOptions,
  listTesterGroupsQueryKey,
} from '@oore/client/react-query'

import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

// ── Tester groups ───────────────────────────────────────────────

export function useTesterGroups(options?: { enabled?: boolean }) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(instanceId, listTesterGroupsOptions({ client })),
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    staleTime: 60_000,
  })
}

export function useSaveTesterGroup() {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: ({ id, body }: { id?: string; body: TesterGroupRequest }) =>
      id
        ? updateTesterGroup({
            body,
            client,
            path: { group_id: id },
            throwOnError: true,
          })
        : createTesterGroup({ body, client, throwOnError: true }),
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listTesterGroupsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useDeleteTesterGroup() {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (id: string) =>
      deleteTesterGroup({ client, path: { group_id: id }, throwOnError: true }),
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listTesterGroupsQueryKey({ client }),
        ),
      })
    },
  })
}

// ── A build's testers ───────────────────────────────────────────

export function useBuildShares(
  buildId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      listBuildSharesOptions({ client, path: { build_id: buildId } }),
    ),
    enabled: !!baseUrl && !!token && !!buildId && (options?.enabled ?? true),
    // Installs show up as testers open their email.
    refetchInterval: 60_000,
  })
}

export function useBuildInstalls(
  buildId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      listBuildInstallsOptions({ client, path: { build_id: buildId } }),
    ),
    enabled: !!baseUrl && !!token && !!buildId && (options?.enabled ?? true),
    refetchInterval: 60_000,
  })
}

export function useSendBuildToGroup(buildId: string) {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (body: SendBuildToGroupRequest) =>
      sendBuildToGroup({
        body,
        client,
        path: { build_id: buildId },
        throwOnError: true,
      }),
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listBuildSharesQueryKey({ client, path: { build_id: buildId } }),
        ),
      })
    },
  })
}

/** Keep a build's files past retention, or let retention have them again. */
export function useKeepBuild(buildId: string) {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (keep: boolean) => {
      const options = {
        client,
        path: { build_id: buildId },
        throwOnError: true as const,
      }
      return keep ? keepBuild(options) : unkeepBuild(options)
    },
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getBuildQueryKey({ client, path: { build_id: buildId } }),
        ),
      })
    },
  })
}

// ── Latest-build links ──────────────────────────────────────────

export function useLatestInstallLinks(pipelineId: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      listLatestInstallLinksOptions({
        client,
        path: { pipeline_id: pipelineId },
      }),
    ),
    enabled: !!baseUrl && !!token && !!pipelineId,
  })
}

export function useCreateLatestInstallLink(pipelineId: string) {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (body: { platform: BuildPlatform; branch?: string | null }) =>
      createLatestInstallLink({
        body,
        client,
        path: { pipeline_id: pipelineId },
        throwOnError: true,
      }),
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listLatestInstallLinksQueryKey({
            client,
            path: { pipeline_id: pipelineId },
          }),
        ),
      })
    },
  })
}

export function useDeleteLatestInstallLink(pipelineId: string) {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  return useMutation({
    mutationFn: (linkId: string) =>
      deleteLatestInstallLink({
        client,
        path: { link_id: linkId },
        throwOnError: true,
      }),
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listLatestInstallLinksQueryKey({
            client,
            path: { pipeline_id: pipelineId },
          }),
        ),
      })
    },
  })
}

// ── Feedback across apps ────────────────────────────────────────

/** Testers' notes from the last two days, for Needs attention. */
export function useRecentFeedback(options?: { enabled?: boolean }) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      listRecentFeedbackOptions({ client, query: { days: 2 } }),
    ),
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    staleTime: 60_000,
    refetchInterval: 5 * 60_000,
  })
}

// ── Registering an iPhone ───────────────────────────────────────

export function useStartDeviceRegistration(pipelineId: string) {
  const { client } = useApiContext()
  return useMutation({
    mutationFn: (deviceName: string | null) =>
      createDeviceRegistration({
        body: { device_name: deviceName },
        client,
        path: { pipeline_id: pipelineId },
        throwOnError: true,
      }),
  })
}

/** Polls a registration until the phone reports back or it expires. */
export function useDeviceRegistration(registrationId: string | null) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getDeviceRegistrationOptions({
        client,
        path: { registration_id: registrationId ?? '' },
      }),
    ),
    enabled: !!baseUrl && !!token && !!registrationId,
    refetchInterval: (query) =>
      query.state.data?.state === 'pending' ? 3000 : false,
  })
}

// ── Browser push ────────────────────────────────────────────────

export function useWebPushConfig(endpoint: string | null) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getWebPushConfigOptions({
        client,
        query: endpoint ? { endpoint } : undefined,
      }),
    ),
    enabled: !!baseUrl && !!token,
    staleTime: 5 * 60_000,
  })
}

export function useWebPushSubscription() {
  const queryClient = useQueryClient()
  const { client, instanceId } = useApiContext()
  const invalidate = () =>
    queryClient.invalidateQueries({
      queryKey: scopeOoreQueryKey(
        instanceId,
        getWebPushConfigQueryKey({ client }),
      ),
    })
  const subscribe = useMutation({
    mutationFn: (body: { endpoint: string; p256dh: string; auth: string }) =>
      subscribeWebPush({ body, client, throwOnError: true }),
    onSettled: invalidate,
  })
  const unsubscribe = useMutation({
    mutationFn: (endpoint: string) =>
      unsubscribeWebPush({ body: { endpoint }, client, throwOnError: true }),
    onSettled: invalidate,
  })
  const test = useMutation({
    mutationFn: () => sendTestWebPush({ client, throwOnError: true }),
  })
  return { subscribe, unsubscribe, test }
}
