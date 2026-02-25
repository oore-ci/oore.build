import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import type { CreateProjectRequest, UpdateProjectRequest } from '@/lib/types'
import {
  createProject,
  deleteProject,
  getProject,
  listProjects,
  updateProject,
} from '@/lib/api'
import {
  useAuthToken,
  useBaseUrl,
  useInstanceQueryPrefix,
} from '@/hooks/query-context'

export function useProjects(params?: {
  search?: string
  limit?: number
  offset?: number
}) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'projects', params ?? {}],
    queryFn: () => listProjects(baseUrl()!, token()!, params),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useProject(projectId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'project', projectId],
    queryFn: () => getProject(baseUrl()!, token()!, projectId),
    enabled: !!baseUrl() && !!token() && !!projectId,
  }))
}

export function useCreateProject() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: CreateProjectRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return createProject(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'projects'] })
    },
  }))
}

export function useUpdateProject() {
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
      data: UpdateProjectRequest
    }) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return updateProject(baseUrl()!, token()!, projectId, data)
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'projects'] })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'project', variables.projectId],
      })
    },
  }))
}

export function useDeleteProject() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (projectId: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return deleteProject(baseUrl()!, token()!, projectId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'projects'] })
    },
  }))
}
