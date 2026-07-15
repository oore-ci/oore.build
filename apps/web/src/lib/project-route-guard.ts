import { redirect } from '@tanstack/react-router'

import { getProject } from '@/lib/api'
import { hasProjectPermission } from '@/hooks/use-permissions'
import { queryClient } from '@/lib/query-client'
import { resolveRequiredInstanceApiBaseUrl } from '@/lib/instance-url'
import type { Instance, ProjectDetailResponse } from '@/lib/types'
import { useAuthStore } from '@/stores/auth-store'

export async function requireProjectPermissionOrRedirect({
  action,
  instance,
  projectId,
  resource,
  token,
}: {
  action: string
  instance: Instance
  projectId: string
  resource: string
  token: string
}): Promise<ProjectDetailResponse> {
  const instanceRole = useAuthStore.getState().user?.role
  if (instanceRole === 'owner' || instanceRole === 'admin') {
    return queryClient.ensureQueryData({
      queryKey: [instance.id, 'project', projectId],
      queryFn: ({ signal }) =>
        getProject(
          resolveRequiredInstanceApiBaseUrl(instance),
          token,
          projectId,
          { signal },
        ),
    })
  }

  if (instanceRole !== 'developer') throw redirect({ to: '/' })

  const project = await queryClient.ensureQueryData({
    queryKey: [instance.id, 'project', projectId],
    queryFn: ({ signal }) =>
      getProject(
        resolveRequiredInstanceApiBaseUrl(instance),
        token,
        projectId,
        { signal },
      ),
  })
  const projectRole =
    project.current_user_role ?? project.project.current_user_role
  if (!hasProjectPermission(projectRole, resource, action)) {
    throw redirect({
      to: '/projects/$projectId',
      params: { projectId },
    })
  }
  return project
}
