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
  BuildStatus,
  CreateBuildRequest,
  CreateScopedDownloadTokenRequest,
  ListBuildsResponse,
} from '@/lib/types'
import {
  cancelBuild,
  createArtifactInstallLink,
  createBuild,
  createScopedDownloadToken,
  getArtifactDownloadLink,
  getBuild,
  getBuildChangelogPreview,
  getBuildLogs,
  listArtifacts,
  listBuildArtifacts,
  listProjectArtifacts,
  listBuilds,
  rerunBuild,
} from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'

const BUILD_POLL_INTERVAL_MS = 3_000

const TERMINAL_STATUSES: Set<string> = new Set<string>([
  'succeeded',
  'failed',
  'canceled',
  'timed_out',
  'expired',
])

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

export function useBuilds(
  params?: {
    project_id?: string
    pipeline_id?: string
    status?: string
    branch?: string
    limit?: number
    offset?: number
  },
  options?: { refetchInterval?: number | false },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()
  const pollInterval = options?.refetchInterval ?? BUILD_POLL_INTERVAL_MS

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'builds', params ?? {}],
    queryFn: ({ signal }) => listBuilds(baseUrl!, token!, params, { signal }),
    enabled: !!baseUrl && !!token,
    staleTime: 5_000,
    refetchInterval: (query) =>
      hasActiveBuilds(query.state.data) ? pollInterval : false,
    placeholderData: keepPreviousData,
  })
}

export function isTerminalStatus(status: BuildStatus | string): boolean {
  return TERMINAL_STATUSES.has(status)
}

export function hasActiveBuilds(data: ListBuildsResponse | undefined): boolean {
  return data?.builds.some((build) => !isTerminalStatus(build.status)) ?? false
}

export function useBuild(
  buildId: string,
  options?: Pick<UseQueryOptions<BuildDetailResponse>, 'refetchInterval'>,
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'build', buildId],
    queryFn: ({ signal }) => getBuild(baseUrl!, token!, buildId, { signal }),
    enabled: !!baseUrl && !!token && !!buildId,
    staleTime: 5_000,
    refetchInterval: options?.refetchInterval,
  })
}

export function useCreateBuild() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()
  const instanceId = instance?.id ?? '__none__'

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
      return createBuild(baseUrl, token, projectId, data)
    },
    onMutate: async ({ projectId, data }) => {
      await queryClient.cancelQueries({
        queryKey: [instanceId, 'builds'],
      })

      const queriesData = queryClient.getQueriesData<ListBuildsResponse>({
        queryKey: [instanceId, 'builds'],
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
        queryKey: [instanceId, 'builds'],
      })
    },
  })
}

export function useBuildChangelogPreview(
  projectId: string,
  params: { pipeline_id: string; branch?: string; commit_sha?: string },
  options?: { enabled?: boolean },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery<BuildChangelogPreviewResponse>({
    queryKey: [
      instance?.id ?? '__none__',
      'build-changelog-preview',
      projectId,
      params,
    ],
    queryFn: ({ signal }) =>
      getBuildChangelogPreview(baseUrl!, token!, projectId, params, { signal }),
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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (buildId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return cancelBuild(baseUrl, token, buildId)
    },
    onSuccess: (_data, buildId) => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'builds'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'build', buildId],
      })
    },
  })
}

export function useRerunBuild() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (buildId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return rerunBuild(baseUrl, token, buildId)
    },
    onSuccess: (_data, buildId) => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'builds'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'build', buildId],
      })
    },
  })
}

export function useBuildLogs(buildId: string, options?: { enabled?: boolean }) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'build-logs', buildId],
    queryFn: async ({ signal }) => {
      // Fetch all log pages (server max per page is 5000)
      const pageSize = 5000
      let allLogs: Array<BuildLogChunk> = []
      let afterSeq = -1
      let total = 0

      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
      while (true) {
        signal.throwIfAborted()
        const page = await getBuildLogs(
          baseUrl!,
          token!,
          buildId,
          {
            after_sequence: afterSeq >= 0 ? afterSeq : undefined,
            limit: pageSize,
          },
          { signal },
        )
        total = page.total
        if (page.logs.length === 0) break
        allLogs = allLogs.concat(page.logs)
        afterSeq = page.logs[page.logs.length - 1].sequence
        // If we got fewer than requested, we've reached the end
        if (page.logs.length < pageSize) break
      }

      return { logs: allLogs, total }
    },
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!buildId,
  })
}

export function useArtifacts(
  buildId: string,
  options?: { refetchInterval?: number | false },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'artifacts', buildId],
    queryFn: ({ signal }) =>
      listArtifacts(baseUrl!, token!, buildId, { signal }),
    enabled: !!baseUrl && !!token && !!buildId,
    staleTime: 5_000,
    refetchInterval: options?.refetchInterval,
  })
}

export function useProjectArtifacts(projectId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'project-artifacts', projectId],
    queryFn: ({ signal }) =>
      listProjectArtifacts(baseUrl!, token!, projectId, { signal }),
    enabled: !!baseUrl && !!token && !!projectId,
    staleTime: 5_000,
  })
}

export function useArtifactsForBuilds(buildIds: Array<string>) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'build-artifacts', buildIds],
    queryFn: ({ signal }) =>
      listBuildArtifacts(baseUrl!, token!, { build_ids: buildIds }, { signal }),
    enabled: !!baseUrl && !!token && buildIds.length > 0,
    staleTime: 5_000,
  })
}

export function useArtifactDownloadLink() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return useMutation({
    mutationFn: (artifactId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return getArtifactDownloadLink(baseUrl, token, artifactId)
    },
  })
}

export function useArtifactInstallLink() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return useMutation({
    mutationFn: (artifactId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createArtifactInstallLink(baseUrl, token, artifactId)
    },
  })
}

export function useCreateScopedDownloadToken() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const instance = useActiveInstance()

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
      return createScopedDownloadToken(baseUrl, token, artifactId, data)
    },
    onSuccess: (_data, { artifactId }) => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'scoped-tokens', artifactId],
      })
    },
  })
}
