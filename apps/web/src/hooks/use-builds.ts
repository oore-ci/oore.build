import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import type { BuildLogChunk, BuildStatus, CreateBuildRequest } from '@/lib/types'
import {
  cancelBuild,
  createBuild,
  getArtifactDownloadLink,
  getBuild,
  getBuildLogs,
  listArtifacts,
  listBuilds,
} from '@/lib/api'
import {
  useAuthToken,
  useBaseUrl,
  useInstanceQueryPrefix,
} from '@/hooks/query-context'

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

export function useBuilds(params?: {
  project_id?: string
  pipeline_id?: string
  status?: string
  branch?: string
  limit?: number
  offset?: number
}) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'builds', params ?? {}],
    queryFn: () => listBuilds(baseUrl()!, token()!, params),
    enabled: !!baseUrl() && !!token(),
    refetchInterval: 5_000,
  }))
}

export function useBuild(buildId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'build', buildId],
    queryFn: () => getBuild(baseUrl()!, token()!, buildId),
    enabled: !!baseUrl() && !!token() && !!buildId,
    refetchInterval: 5_000,
  }))
}

export function useBuildLogs(buildId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'build-logs', buildId],
    queryFn: async () => {
      const pageSize = 5_000
      let allLogs: Array<BuildLogChunk> = []
      let afterSeq = -1
      let total = 0

      while (true) {
        const page = await getBuildLogs(baseUrl()!, token()!, buildId, {
          after_sequence: afterSeq >= 0 ? afterSeq : undefined,
          limit: pageSize,
        })
        total = page.total
        if (page.logs.length === 0) break

        allLogs = allLogs.concat(page.logs)
        afterSeq = page.logs[page.logs.length - 1].sequence
        if (page.logs.length < pageSize) break
      }

      return { logs: allLogs, total }
    },
    enabled: !!baseUrl() && !!token() && !!buildId,
    refetchInterval: 2_500,
  }))
}

export function useArtifacts(buildId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'artifacts', buildId],
    queryFn: () => listArtifacts(baseUrl()!, token()!, buildId),
    enabled: !!baseUrl() && !!token() && !!buildId,
    refetchInterval: 3_000,
  }))
}

export function useArtifactDownloadLink() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return createMutation(() => ({
    mutationFn: async (artifactId: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return getArtifactDownloadLink(baseUrl()!, token()!, artifactId)
    },
  }))
}

export function useCreateBuild() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async ({
      projectId,
      data,
    }: {
      projectId: string
      data: CreateBuildRequest
    }) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return createBuild(baseUrl()!, token()!, projectId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'builds'] })
    },
  }))
}

export function useCancelBuild() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (buildId: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return cancelBuild(baseUrl()!, token()!, buildId)
    },
    onSuccess: (_data, buildId) => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'builds'] })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'build', buildId],
      })
    },
  }))
}
