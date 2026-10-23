import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type {
  CreateProjectEnvironmentRequest,
  CreatePromotionRequest,
  ListProjectEnvironmentsResponse,
  UpdateProjectEnvironmentRequest,
} from '@oore/client/models'
import {
  approvePromotion,
  createProjectEnvironment,
  deleteProjectEnvironment,
  promoteBuild,
  rejectPromotion,
  updateProjectEnvironment,
} from '@oore/client/operations'
import {
  listProjectEnvironmentsOptions,
  listProjectEnvironmentsQueryKey,
  listReleaseEventsQueryKey,
  listReleasesQueryKey,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import { RELEASE_POLL_INTERVAL_MS } from '@/hooks/use-releases'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'
import { isMovingRelease } from '@/lib/releases'

function isMoving(data: ListProjectEnvironmentsResponse | undefined) {
  return (
    !!data &&
    (data.environments.some(
      (environment) =>
        environment.latest_release &&
        isMovingRelease(environment.latest_release),
    ) ||
      data.promotions.some((promotion) => promotion.state === 'promoting'))
  )
}

export function useProjectEnvironments(
  projectId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listProjectEnvironmentsOptions({
      client,
      path: { project_id: projectId },
    }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && (options?.enabled ?? true),
    staleTime: 2_000,
    // Rollouts move on their own; pending promotions wait for a person.
    refetchInterval: (query) =>
      isMoving(query.state.data) ? RELEASE_POLL_INTERVAL_MS * 5 : 30_000,
  })
}

type OoreClient = ReturnType<typeof useApiContext>['client']

/** Mutations here change environments and may create releases. */
function useEnvironmentMutation<TVariables, TResult>(
  projectId: string,
  run: (client: OoreClient, variables: TVariables) => Promise<TResult>,
) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: async (variables: TVariables) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return run(client, variables)
    },
    onSettled: () => {
      for (const queryKey of [
        listProjectEnvironmentsQueryKey({
          client,
          path: { project_id: projectId },
        }),
        listReleasesQueryKey({ client }),
        listReleaseEventsQueryKey({ client }),
      ]) {
        void queryClient.invalidateQueries({
          queryKey: scopeOoreQueryKey(instanceId, queryKey),
        })
      }
    },
  })
}

export function useCreateEnvironment(projectId: string) {
  return useEnvironmentMutation(
    projectId,
    (client, body: CreateProjectEnvironmentRequest) =>
      createProjectEnvironment({
        body,
        client,
        path: { project_id: projectId },
      }),
  )
}

export function useUpdateEnvironment(projectId: string) {
  return useEnvironmentMutation(
    projectId,
    (
      client,
      {
        body,
        environmentId,
      }: { body: UpdateProjectEnvironmentRequest; environmentId: string },
    ) =>
      updateProjectEnvironment({
        body,
        client,
        path: { project_id: projectId, environment_id: environmentId },
      }),
  )
}

export function useDeleteEnvironment(projectId: string) {
  return useEnvironmentMutation(projectId, (client, environmentId: string) =>
    deleteProjectEnvironment({
      client,
      path: { project_id: projectId, environment_id: environmentId },
    }),
  )
}

export function usePromoteBuild(projectId: string) {
  return useEnvironmentMutation(
    projectId,
    (client, body: CreatePromotionRequest) =>
      promoteBuild({ body, client, path: { project_id: projectId } }),
  )
}

export function useDecidePromotion(projectId: string) {
  return useEnvironmentMutation(
    projectId,
    (
      client,
      {
        approve,
        promotionId,
        reason,
      }: { approve: boolean; promotionId: string; reason?: string },
    ) => {
      const options = {
        body: reason ? { reason } : {},
        client,
        path: { project_id: projectId, promotion_id: promotionId },
      }
      return approve ? approvePromotion(options) : rejectPromotion(options)
    },
  )
}
