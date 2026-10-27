import {
  useMutation,
  useQuery,
  useQueryClient,
  type Query,
} from '@tanstack/react-query'
import * as z from 'zod'

import type {
  CreateSigningIdentityRequest,
  ListPipelineWorkflowSettingsResponse,
  ListSigningIdentitiesResponse,
  UpdatePipelineWorkflowSettingsRequest,
  UpdateSigningIdentityRequest,
} from '@oore/client/models'
import {
  createSigningIdentity,
  deleteSigningIdentity,
  updatePipelineWorkflowSettings,
  updateSigningIdentity,
} from '@oore/client/operations'
import {
  listPipelineWorkflowSettingsOptions,
  listSigningIdentitiesOptions,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import { scopeOoreQueryOptions } from '@/lib/api-client/client'

export type {
  PipelineWorkflowSettings,
  SigningIdentity,
  SigningIdentityKind,
} from '@oore/client/models'

// An identity lists the workflow settings that pick it, and the settings
// name identities, so a change to either refreshes both.
const SIGNING_OPERATIONS = new Set([
  'listSigningIdentities',
  'listPipelineWorkflowSettings',
])

const generatedQueryKeySchema = z.object({ _id: z.string() })

function isSigningQuery(query: Query) {
  return query.queryKey.some((part) => {
    const operation = generatedQueryKeySchema.safeParse(part).data?._id
    return !!operation && SIGNING_OPERATIONS.has(operation)
  })
}

function selectIdentities(response: ListSigningIdentitiesResponse) {
  return response.identities
}

function selectSettings(response: ListPipelineWorkflowSettingsResponse) {
  return response.settings
}

/** The project's signing library. Key material is never returned. */
export function useSigningIdentities(
  projectId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      listSigningIdentitiesOptions({
        client,
        path: { project_id: projectId },
      }),
    ),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!projectId,
    select: selectIdentities,
  })
}

/** Settings the pipeline keeps per workflow; workflows without any are absent. */
export function usePipelineWorkflowSettings(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      listPipelineWorkflowSettingsOptions({
        client,
        path: { pipeline_id: pipelineId },
      }),
    ),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token && !!pipelineId,
    select: selectSettings,
  })
}

type OoreClient = ReturnType<typeof useApiContext>['client']

function useSigningMutation<TVariables, TResult>(
  run: (client: OoreClient, variables: TVariables) => Promise<TResult>,
) {
  const queryClient = useQueryClient()
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: async (variables: TVariables) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return run(client, variables)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ predicate: isSigningQuery })
    },
  })
}

export function useCreateSigningIdentity(projectId: string) {
  return useSigningMutation(
    async (client, body: CreateSigningIdentityRequest) =>
      (
        await createSigningIdentity({
          body,
          client,
          path: { project_id: projectId },
        })
      ).identity,
  )
}

export function useUpdateSigningIdentity(projectId: string) {
  return useSigningMutation(
    async (
      client,
      { id, ...body }: UpdateSigningIdentityRequest & { id: string },
    ) =>
      (
        await updateSigningIdentity({
          body,
          client,
          path: { project_id: projectId, identity_id: id },
        })
      ).identity,
  )
}

export function useDeleteSigningIdentity(projectId: string) {
  return useSigningMutation((client, id: string) =>
    deleteSigningIdentity({
      client,
      path: { project_id: projectId, identity_id: id },
    }),
  )
}

/** Replaces one workflow's settings; omitted fields are cleared. */
export function useUpdatePipelineWorkflowSettings(pipelineId: string) {
  return useSigningMutation(
    (
      client,
      {
        workflow,
        ...body
      }: UpdatePipelineWorkflowSettingsRequest & { workflow: string },
    ) =>
      updatePipelineWorkflowSettings({
        body,
        client,
        path: { pipeline_id: pipelineId, workflow },
      }),
  )
}
