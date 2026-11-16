import {
  useMutation,
  useQuery,
  useQueryClient,
  type Query,
} from '@tanstack/react-query'
import * as z from 'zod'

import type {
  ProjectStoresResponse,
  UpdateProjectAppStoreConnectionRequest,
  UpdateProjectGooglePlayConnectionRequest,
} from '@oore/client/models'
import {
  deleteProjectAppStoreConnection,
  deleteProjectGooglePlayConnection,
  testProjectAppStoreConnection,
  testProjectGooglePlayConnection,
  updateProjectAppStoreConnection,
  updateProjectGooglePlayConnection,
} from '@oore/client/operations'
import {
  getProjectStoresOptions,
  getProjectStoresQueryKey,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

export type {
  AppStoreConnectionTestResponse,
  ProjectAppStoreConnection,
  ProjectGooglePlayConnection,
  ProjectStoreOverride,
  ProjectStoresResponse,
} from '@oore/client/models'

/** The app's Google Play and App Store Connect connections. */
export function useProjectStores(
  projectId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getProjectStoresOptions({ client, path: { project_id: projectId } }),
    ),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
  })
}

// Pipeline publish responses resolve through the app's connections.
const STORE_DEPENDENT_OPERATIONS = new Set([
  'getPipelineGooglePlayPublish',
  'getPipelineAppStorePublish',
  'getPipelinePublishReadiness',
  'listAppStoreConnectKeys',
])

const generatedQueryKeySchema = z.object({ _id: z.string() })

function isStoreDependentQuery(query: Query) {
  return query.queryKey.some((part) => {
    const operation = generatedQueryKeySchema.safeParse(part).data?._id
    return !!operation && STORE_DEPENDENT_OPERATIONS.has(operation)
  })
}

type OoreClient = ReturnType<typeof useApiContext>['client']

function useStoreMutation<TVariables>(
  projectId: string,
  run: (
    client: OoreClient,
    variables: TVariables,
  ) => Promise<ProjectStoresResponse>,
) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useMutation({
    mutationFn: async (variables: TVariables) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return run(client, variables)
    },
    onSuccess: (response) => {
      queryClient.setQueryData(
        scopeOoreQueryKey(
          instanceId,
          getProjectStoresQueryKey({
            client,
            path: { project_id: projectId },
          }),
        ),
        response,
      )
      void queryClient.invalidateQueries({ predicate: isStoreDependentQuery })
    },
  })
}

export function useConnectGooglePlay(projectId: string) {
  return useStoreMutation(
    projectId,
    (client, body: UpdateProjectGooglePlayConnectionRequest) =>
      updateProjectGooglePlayConnection({
        body,
        client,
        path: { project_id: projectId },
      }),
  )
}

export function useDisconnectGooglePlay(projectId: string) {
  return useStoreMutation<void>(projectId, (client) =>
    deleteProjectGooglePlayConnection({
      client,
      path: { project_id: projectId },
    }),
  )
}

export function useConnectAppStore(projectId: string) {
  return useStoreMutation(
    projectId,
    (client, body: UpdateProjectAppStoreConnectionRequest) =>
      updateProjectAppStoreConnection({
        body,
        client,
        path: { project_id: projectId },
      }),
  )
}

export function useDisconnectAppStore(projectId: string) {
  return useStoreMutation<void>(projectId, (client) =>
    deleteProjectAppStoreConnection({
      client,
      path: { project_id: projectId },
    }),
  )
}

/** A failed test is an answer with `ok: false`, not an error. */
export function useTestGooglePlayConnection(projectId: string) {
  const { baseUrl, client, token } = useApiContext()
  return useMutation({
    mutationFn: async (body: UpdateProjectGooglePlayConnectionRequest) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return testProjectGooglePlayConnection({
        body,
        client,
        path: { project_id: projectId },
      })
    },
  })
}

export function useTestAppStoreConnection(projectId: string) {
  const { baseUrl, client, token } = useApiContext()
  return useMutation({
    mutationFn: async (body: UpdateProjectAppStoreConnectionRequest) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return testProjectAppStoreConnection({
        body,
        client,
        path: { project_id: projectId },
      })
    },
  })
}
