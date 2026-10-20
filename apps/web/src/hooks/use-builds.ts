import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import type { UseQueryOptions } from '@tanstack/react-query'
import type {
  Build,
  BuildChangelogPreviewResponse,
  BuildDetailResponse,
  BuildLogChunk,
  BuildLogsResponse,
  BuildStatus,
  CreateBuildRequest,
  CreateScopedDownloadTokenRequest,
  ListBuildsResponse,
} from '@oore/client/models'
import {
  cancelBuild,
  createBuild,
  rerunBuild,
  createArtifactInstallLink,
  generateDownloadLink as getArtifactDownloadLink,
  listBuildArtifacts,
  createScopedDownloadToken,
  getBuildLogs,
} from '@oore/client/operations'
import {
  getBuildOptions,
  getBuildLogsQueryKey,
  getBuildQueryKey,
  listArtifactsOptions,
  listBuildsOptions,
  listBuildsQueryKey,
  listBuildArtifactsMutationKey,
  listProjectArtifactsOptions,
  previewBuildChangelogOptions,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

const BUILD_POLL_INTERVAL_MS = 3_000

const TERMINAL_STATUSES: Set<string> = new Set<string>([
  'succeeded',
  'failed',
  'canceled',
  'timed_out',
  'expired',
])

export function useBuilds<TData = ListBuildsResponse>(
  params?: {
    project_id?: string
    pipeline_id?: string
    status?: string
    branch?: string
    sort?: 'created_at' | 'status' | 'project_name' | 'pipeline_name' | 'branch'
    direction?: 'asc' | 'desc'
    limit?: number
    offset?: number
  },
  options?: {
    enabled?: boolean
    refetchInterval?: number | false
    select?: (data: ListBuildsResponse) => TData
  },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const pollInterval = options?.refetchInterval ?? BUILD_POLL_INTERVAL_MS
  const query = scopeOoreQueryOptions(
    instanceId,
    listBuildsOptions({ client, query: params }),
  )

  return useQuery<ListBuildsResponse, Error, TData>({
    ...query,
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    staleTime: 5_000,
    refetchInterval: (query) =>
      hasActiveBuilds(query.state.data) ? pollInterval : false,
    placeholderData: keepPreviousData,
    select: options?.select,
  })
}

export function isTerminalStatus(status: BuildStatus): boolean {
  return TERMINAL_STATUSES.has(status)
}

export function hasActiveBuilds(data: ListBuildsResponse | undefined): boolean {
  return data?.builds.some((build) => !isTerminalStatus(build.status)) ?? false
}

export function useBuild(
  buildId: string,
  options?: Pick<UseQueryOptions<BuildDetailResponse>, 'refetchInterval'>,
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getBuildOptions({ client, path: { build_id: buildId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!buildId,
    staleTime: 5_000,
    refetchInterval: options?.refetchInterval,
  })
}

export function useCreateBuild() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()
  const buildsQueryKey = scopeOoreQueryKey(
    instanceId,
    listBuildsQueryKey({ client }),
  )

  return useMutation({
    mutationFn: ({
      projectId,
      data,
    }: {
      projectId: string
      data: CreateBuildRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createBuild({
        body: data,
        client,
        path: { project_id: projectId },
      })
    },
    onMutate: async ({ projectId, data }) => {
      await queryClient.cancelQueries({
        queryKey: buildsQueryKey,
      })

      const queriesData = queryClient.getQueriesData<ListBuildsResponse>({
        queryKey: buildsQueryKey,
      })

      const optimisticBuild: Build = {
        id: `optimistic-${Date.now()}`,
        project_id: projectId,
        pipeline_id: data.pipeline_id,
        build_number: 0,
        status: 'queued',
        trigger_type: 'manual',
        branch: data.branch,
        commit_sha: data.commit_sha,
        trigger_ref: data.trigger_ref,
        changelog: data.changelog,
        config_snapshot: {},
        queued_at: Math.floor(Date.now() / 1000),
        created_at: Math.floor(Date.now() / 1000),
        updated_at: Math.floor(Date.now() / 1000),
      }

      for (const [key, existing] of queriesData) {
        if (existing) {
          queryClient.setQueryData(key, {
            builds: [optimisticBuild, ...existing.builds],
            total: existing.total + 1,
          })
        }
      }

      return { queriesData }
    },
    onError: (_err, _vars, context) => {
      if (context?.queriesData) {
        for (const [key, data] of context.queriesData) {
          queryClient.setQueryData(key, data)
        }
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: buildsQueryKey,
      })
    },
  })
}

export function useBuildChangelogPreview(
  projectId: string,
  params: { pipeline_id: string; branch?: string; commit_sha?: string },
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    previewBuildChangelogOptions({
      client,
      path: { project_id: projectId },
      query: params,
    }),
  )

  return useQuery<BuildChangelogPreviewResponse>({
    ...query,
    enabled:
      !!baseUrl &&
      !!token &&
      (options?.enabled ?? true) &&
      !!projectId &&
      !!params.pipeline_id &&
      (!!params.branch || !!params.commit_sha),
    staleTime: 30_000,
    retry: false,
  })
}

export function useCancelBuild() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (buildId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return cancelBuild({ client, path: { build_id: buildId } })
    },
    onSuccess: (_data, buildId) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(instanceId, listBuildsQueryKey({ client })),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getBuildQueryKey({ client, path: { build_id: buildId } }),
        ),
      })
    },
  })
}

export function useRerunBuild() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (buildId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return rerunBuild({ client, path: { build_id: buildId } })
    },
    onSuccess: (_data, buildId) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(instanceId, listBuildsQueryKey({ client })),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getBuildQueryKey({ client, path: { build_id: buildId } }),
        ),
      })
    },
  })
}

export function useBuildLogs(buildId: string, options?: { enabled?: boolean }) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    queryKey: scopeOoreQueryKey(
      instanceId,
      getBuildLogsQueryKey({ client, path: { build_id: buildId } }),
    ),
    queryFn: async ({ signal }) => {
      const pageSize = 5000
      const logs: Array<BuildLogChunk> = []
      let afterSeq = -1
      let page: BuildLogsResponse

      do {
        signal.throwIfAborted()
        page = await getBuildLogs({
          client,
          path: { build_id: buildId },
          query: {
            after_sequence: afterSeq >= 0 ? afterSeq : undefined,
            limit: pageSize,
          },
          signal,
        })
        logs.push(...page.logs)
        afterSeq = page.logs.at(-1)?.sequence ?? afterSeq
      } while (page.logs.length === pageSize)

      return { logs, total: page.total }
    },
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!buildId,
  })
}

export function useArtifacts(
  buildId: string,
  options?: { refetchInterval?: number | false },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listArtifactsOptions({ client, path: { build_id: buildId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!buildId,
    staleTime: 5_000,
    refetchInterval: options?.refetchInterval,
  })
}

export function useProjectArtifacts(projectId: string, limit = 50) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listProjectArtifactsOptions({
      client,
      path: { project_id: projectId },
      query: { limit },
    }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!projectId,
    staleTime: 5_000,
  })
}

export function useArtifactsForBuilds(buildIds: Array<string>) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    queryKey: scopeOoreQueryKey(
      instanceId,
      listBuildArtifactsMutationKey({
        client,
        body: { build_ids: buildIds },
      }),
    ),
    queryFn: ({ signal }) =>
      listBuildArtifacts({ body: { build_ids: buildIds }, client, signal }),
    enabled: !!baseUrl && !!token && buildIds.length > 0,
    staleTime: 5_000,
  })
}

export function useArtifactDownloadLink() {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: async (artifactId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      const response = await getArtifactDownloadLink({
        client,
        path: { artifact_id: artifactId },
      })
      return {
        ...response,
        download_url: new URL(response.download_url, baseUrl).href,
      }
    },
  })
}

export function useArtifactInstallLink() {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: async (artifactId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      const response = await createArtifactInstallLink({
        client,
        path: { artifact_id: artifactId },
      })
      return {
        ...response,
        download_url: new URL(response.download_url, baseUrl).href,
        install_url: new URL(response.install_url, baseUrl).href,
      }
    },
  })
}

export function useCreateScopedDownloadToken() {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: ({
      artifactId,
      data,
    }: {
      artifactId: string
      data: CreateScopedDownloadTokenRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createScopedDownloadToken({
        body: data,
        client,
        path: { artifact_id: artifactId },
      })
    },
  })
}
