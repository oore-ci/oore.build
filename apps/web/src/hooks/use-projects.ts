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
  ListProjectsData,
  UpdateProjectMemberRequest,
  UpdateProjectRequest,
} from '@oore/client/models'
import {
  addProjectMember,
  createProject,
  deleteProject,
  removeProjectMember,
  updateProject,
  updateProjectMember,
} from '@oore/client/operations'
import {
  getProjectOptions,
  getProjectQueryKey,
  listProjectMemberCandidatesOptions,
  listProjectMemberCandidatesQueryKey,
  listProjectMembersOptions,
  listProjectMembersQueryKey,
  listProjectsInfiniteOptions,
  listProjectsOptions,
  listProjectsQueryKey,
} from '@oore/client/react-query'

import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreInfiniteQueryOptions,
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

type ListProjectsParams = NonNullable<ListProjectsData['query']>

export function useInfiniteProjects(
  params?: ListProjectsParams,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const enabled = options?.enabled ?? true
  const query = scopeOoreInfiniteQueryOptions(
    instanceId,
    listProjectsInfiniteOptions({
      client,
      query: { ...params, limit: params?.limit ?? 100 },
    }),
  )

  return useInfiniteQuery({
    ...query,
    initialPageParam: 0,
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

export function useProjects(
  params?: ListProjectsParams,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const enabled = options?.enabled ?? true
  const query = scopeOoreQueryOptions(
    instanceId,
    listProjectsOptions({ client, query: params }),
  )

  return useQuery({
    ...query,
    enabled: enabled && !!baseUrl && !!token,
    placeholderData: keepPreviousData,
  })
}

export function useProject(projectId: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getProjectOptions({
      client,
      path: { project_id: projectId },
    }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!projectId,
  })
}

export function useCreateProject() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (data: CreateProjectRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createProject({ body: data, client, throwOnError: true })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useUpdateProject() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

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
      return updateProject({
        body: data,
        client,
        path: { project_id: projectId },
        throwOnError: true,
      })
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectsQueryKey({ client }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getProjectQueryKey({
            client,
            path: { project_id: variables.projectId },
          }),
        ),
      })
    },
  })
}

export function useDeleteProject() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (projectId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deleteProject({
        client,
        path: { project_id: projectId },
        throwOnError: true,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useProjectMembers(projectId: string, enabled = true) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listProjectMembersOptions({
      client,
      path: { project_id: projectId },
    }),
  )

  return useQuery({
    ...query,
    enabled: enabled && !!baseUrl && !!token && !!projectId,
  })
}

export function useProjectMemberCandidates(projectId: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listProjectMemberCandidatesOptions({
      client,
      path: { project_id: projectId },
    }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!projectId,
  })
}

export function useAddProjectMember(projectId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (data: AddProjectMemberRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return addProjectMember({
        body: data,
        client,
        path: { project_id: projectId },
        throwOnError: true,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectMembersQueryKey({
            client,
            path: { project_id: projectId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectMemberCandidatesQueryKey({
            client,
            path: { project_id: projectId },
          }),
        ),
      })
    },
  })
}

export function useUpdateProjectMember(projectId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

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
      return updateProjectMember({
        body: data,
        client,
        path: { project_id: projectId, user_id: userId },
        throwOnError: true,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectMembersQueryKey({
            client,
            path: { project_id: projectId },
          }),
        ),
      })
    },
  })
}

export function useRemoveProjectMember(projectId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (userId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return removeProjectMember({
        client,
        path: { project_id: projectId, user_id: userId },
        throwOnError: true,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectMembersQueryKey({
            client,
            path: { project_id: projectId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listProjectMemberCandidatesQueryKey({
            client,
            path: { project_id: projectId },
          }),
        ),
      })
    },
  })
}
