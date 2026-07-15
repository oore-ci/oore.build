import {
  keepPreviousData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import type {
  AddProjectMemberRequest,
  CreateProjectRequest,
  UpdateProjectMemberRequest,
  UpdateProjectRequest,
} from '@/lib/types'
import {
  addProjectMember,
  createProject,
  deleteProject,
  getProject,
  listProjectMemberCandidates,
  listProjectMembers,
  listProjects,
  removeProjectMember,
  updateProjectMember,
  updateProject,
} from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'

function useAuthToken(): string | null {
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  if (!token || expiresAt == null) return null
  if (expiresAt <= Math.floor(Date.now() / 1000)) return null
  return token
}

export function useProjectPages(
  params?: {
    search?: string
    sort?: 'created_at' | 'updated_at' | 'name'
    direction?: 'asc' | 'desc'
    limit?: number
  },
  options?: { enabled?: boolean },
) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()
  const enabled = options?.enabled ?? true
  const limit = params?.limit ?? 20

  return useInfiniteQuery({
    queryKey: [instance?.id ?? '__none__', 'project-pages', params ?? {}],
    initialPageParam: 0,
    queryFn: ({ pageParam, signal }) =>
      listProjects(
        baseUrl!,
        token!,
        { ...params, limit, offset: pageParam },
        { signal },
      ),
    getNextPageParam: (lastPage, allPages) => {
      const loaded = allPages.reduce(
        (count, page) => count + page.projects.length,
        0,
      )
      return loaded < lastPage.total ? loaded : undefined
    },
    enabled: enabled && !!baseUrl && !!token,
  })
}

function useBaseUrl(): string | null {
  const instance = useActiveInstance()
  return resolveInstanceApiBaseUrl(instance)
}

export function useProjects(
  params?: {
    search?: string
    sort?: 'created_at' | 'updated_at' | 'name'
    direction?: 'asc' | 'desc'
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
    queryFn: ({ signal }) => listProjects(baseUrl!, token!, params, { signal }),
    enabled: enabled && !!baseUrl && !!token,
    placeholderData: keepPreviousData,
  })
}

export function useProject(projectId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'project', projectId],
    queryFn: ({ signal }) =>
      getProject(baseUrl!, token!, projectId, { signal }),
    enabled: !!baseUrl && !!token && !!projectId,
  })
}

export function useCreateProject() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: CreateProjectRequest) => {
      if (!baseUrl || !token)
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
      if (!baseUrl || !token)
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
      if (!baseUrl || !token)
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

export function useProjectMembers(projectId: string, enabled = true) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'project-members', projectId],
    queryFn: ({ signal }) =>
      listProjectMembers(baseUrl!, token!, projectId, { signal }),
    enabled: enabled && !!baseUrl && !!token && !!projectId,
  })
}

export function useProjectMemberCandidates(projectId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'project-member-candidates',
      projectId,
    ],
    queryFn: ({ signal }) =>
      listProjectMemberCandidates(baseUrl!, token!, projectId, { signal }),
    enabled: !!baseUrl && !!token && !!projectId,
  })
}

export function useAddProjectMember(projectId: string) {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: AddProjectMemberRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return addProjectMember(baseUrl, token, projectId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'project-members', projectId],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'project-member-candidates',
          projectId,
        ],
      })
    },
  })
}

export function useUpdateProjectMember(projectId: string) {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      userId,
      data,
    }: {
      userId: string
      data: UpdateProjectMemberRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updateProjectMember(baseUrl, token, projectId, userId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'project-members', projectId],
      })
    },
  })
}

export function useRemoveProjectMember(projectId: string) {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (userId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return removeProjectMember(baseUrl, token, projectId, userId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'project-members', projectId],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'project-member-candidates',
          projectId,
        ],
      })
    },
  })
}
