import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { CreateProjectRequest, UpdateProjectRequest } from '@/lib/types'
import {
  createProject,
  deleteProject,
  getProject,
  listProjects,
  updateProject,
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

export function useProjects(
  params?: {
    search?: string
    limit?: number
    offset?: number
  },
  options?: { enabled?: boolean },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()
  const enabled = options?.enabled ?? true

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'projects', params ?? {}],
    queryFn: () => listProjects(baseUrl!, token!, params),
    enabled: enabled && baseUrl !== null && !!token,
  })
}

export function useProject(projectId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'project', projectId],
    queryFn: () => getProject(baseUrl!, token!, projectId),
    enabled: baseUrl !== null && !!token && !!projectId,
  })
}

export function useCreateProject() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: CreateProjectRequest) => {
      if (baseUrl === null || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createProject(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'projects'],
      })
    },
  })
}

export function useUpdateProject() {
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
      data: UpdateProjectRequest
    }) => {
      if (baseUrl === null || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updateProject(baseUrl, token, projectId, data)
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'projects'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'project', variables.projectId],
      })
    },
  })
}

export function useDeleteProject() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (projectId: string) => {
      if (baseUrl === null || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deleteProject(baseUrl, token, projectId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'projects'],
      })
    },
  })
}
