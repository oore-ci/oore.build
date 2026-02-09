import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { BuildStatus, CreateBuildRequest } from '@/lib/types'
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
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'builds', params ?? {}],
    queryFn: () => listBuilds(baseUrl!, token!, params),
    enabled: !!baseUrl && !!token,
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

export function useBuild(buildId: string, options?: { refetchInterval?: number | false }) {
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
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'builds'],
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
      let allLogs: import('@/lib/types').BuildLogChunk[] = []
      let afterSeq = -1
      let total = 0

      // eslint-disable-next-line no-constant-condition
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
