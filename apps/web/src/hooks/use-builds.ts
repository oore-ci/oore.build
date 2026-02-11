import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  Build,
  BuildLogChunk,
  BuildStatus,
  CreateBuildRequest,
  ListBuildsResponse,
} from '@/lib/types'
import {
  cancelBuild,
  createBuild,
  getArtifactDownloadLink,
  getBuild,
  getBuildLogs,
  listArtifacts,
  listBuilds,
} from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

function useAuthToken(): string | null {
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  if (!token || expiresAt == null) return null
  if (expiresAt <= Math.floor(Date.now() / 1000)) return null
  return token
}

function useBaseUrl(): string | null {
  const instance = useActiveInstance()
  return instance?.url ?? null
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

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'builds', params ?? {}],
    queryFn: () => listBuilds(baseUrl!, token!, params),
    enabled: !!baseUrl && !!token,
    refetchInterval: options?.refetchInterval,
  })
}

const TERMINAL_STATUSES: Set<string> = new Set<string>([
  'succeeded',
  'failed',
  'canceled',
  'timed_out',
  'expired',
])

export function isTerminalStatus(status: BuildStatus | string): boolean {
  return TERMINAL_STATUSES.has(status)
}

export function useBuild(
  buildId: string,
  options?: { refetchInterval?: number | false },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'build', buildId],
    queryFn: () => getBuild(baseUrl!, token!, buildId),
    enabled: !!baseUrl && !!token && !!buildId,
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

      const queriesData = queryClient.getQueriesData<
        ListBuildsResponse
      >({ queryKey: [instanceId, 'builds'] })

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

export function useBuildLogs(buildId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'build-logs', buildId],
    queryFn: async () => {
      // Fetch all log pages (server max per page is 5000)
      const pageSize = 5000
      let allLogs: Array<BuildLogChunk> = []
      let afterSeq = -1
      let total = 0

      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
      while (true) {
        const page = await getBuildLogs(baseUrl!, token!, buildId, {
          after_sequence: afterSeq >= 0 ? afterSeq : undefined,
          limit: pageSize,
        })
        total = page.total
        if (page.logs.length === 0) break
        allLogs = allLogs.concat(page.logs)
        afterSeq = page.logs[page.logs.length - 1].sequence
        // If we got fewer than requested, we've reached the end
        if (page.logs.length < pageSize) break
      }

      return { logs: allLogs, total }
    },
    enabled: !!baseUrl && !!token && !!buildId,
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
    queryFn: () => listArtifacts(baseUrl!, token!, buildId),
    enabled: !!baseUrl && !!token && !!buildId,
    refetchInterval: options?.refetchInterval,
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
