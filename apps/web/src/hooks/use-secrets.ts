import {
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
  type Query,
} from '@tanstack/react-query'
import * as z from 'zod'

import type { SecretSummary } from '@oore/client/models'
import {
  createInstanceSecret,
  createPipelineSecret,
  createProjectSecret,
  deleteInstanceSecret,
  deletePipelineSecret,
  deleteProjectSecret,
  rotateInstanceSecret,
  rotatePipelineSecret,
  rotateProjectSecret,
} from '@oore/client/operations'
import {
  listInstanceSecretsOptions,
  listPipelineSecretsOptions,
  listProjectSecretsOptions,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import { scopeOoreQueryOptions } from '@/lib/api-client/client'

export type { SecretScope, SecretSummary } from '@oore/client/models'

export type SecretTarget =
  | { scope: 'instance' }
  | { scope: 'project'; projectId: string }
  | { scope: 'pipeline'; pipelineId: string }

export const SECRET_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/
export const SECRET_NAME_MAX_LENGTH = 128
export const SECRET_VALUE_MAX_BYTES = 64 * 1024

// Generated query keys carry `{ _id: '<operation>' }`. Publish settings report
// whether their referenced secret exists, so they depend on secrets too.
const SECRET_DEPENDENT_OPERATIONS = new Set([
  'listInstanceSecrets',
  'listProjectSecrets',
  'listPipelineSecrets',
  'getPipelineGooglePlayPublish',
])

const generatedQueryKeySchema = z.object({ _id: z.string() })

function isSecretDependentQuery(query: Query) {
  return query.queryKey.some((part) => {
    const operation = generatedQueryKeySchema.safeParse(part).data?._id
    return !!operation && SECRET_DEPENDENT_OPERATIONS.has(operation)
  })
}

function selectSecrets(response: { secrets: Array<SecretSummary> }) {
  return response.secrets
}

function secretsQueryOptions(
  { client, instanceId }: ReturnType<typeof useApiContext>,
  target: SecretTarget,
) {
  switch (target.scope) {
    case 'instance':
      return scopeOoreQueryOptions(
        instanceId,
        listInstanceSecretsOptions({ client }),
      )
    case 'project':
      return scopeOoreQueryOptions(
        instanceId,
        listProjectSecretsOptions({
          client,
          path: { project_id: target.projectId },
        }),
      )
    case 'pipeline':
      return scopeOoreQueryOptions(
        instanceId,
        listPipelineSecretsOptions({
          client,
          path: { pipeline_id: target.pipelineId },
        }),
      )
  }
}

function useSecretsQueryFactory(enabled: boolean) {
  const context = useApiContext()
  return (target: SecretTarget) => ({
    ...secretsQueryOptions(context, target),
    enabled: enabled && !!context.baseUrl && !!context.token,
    select: selectSecrets,
  })
}

export function useSecrets(
  target: SecretTarget,
  options?: { enabled?: boolean },
) {
  const secretsQuery = useSecretsQueryFactory(options?.enabled ?? true)
  return useQuery(secretsQuery(target))
}

/**
 * Secrets from broader scopes, narrowest first. Scopes the caller cannot
 * read are left out rather than failing the whole list.
 */
export function useInheritedSecrets(targets: Array<SecretTarget>) {
  const secretsQuery = useSecretsQueryFactory(true)
  return useQueries({
    queries: targets.map(secretsQuery),
    combine: (results) => results.flatMap((result) => result.data ?? []),
  })
}

type OoreClient = ReturnType<typeof useApiContext>['client']

function useSecretMutation<TVariables, TResult>(
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
      void queryClient.invalidateQueries({ predicate: isSecretDependentQuery })
    },
  })
}

interface SecretValue {
  name: string
  value: string
}

export function useCreateSecret(target: SecretTarget) {
  return useSecretMutation((client, { name, value }: SecretValue) => {
    const body = { name, value }
    switch (target.scope) {
      case 'instance':
        return createInstanceSecret({ body, client })
      case 'project':
        return createProjectSecret({
          body,
          client,
          path: { project_id: target.projectId },
        })
      case 'pipeline':
        return createPipelineSecret({
          body,
          client,
          path: { pipeline_id: target.pipelineId },
        })
    }
  })
}

export function useRotateSecret(target: SecretTarget) {
  return useSecretMutation((client, { name, value }: SecretValue) => {
    const body = { value }
    switch (target.scope) {
      case 'instance':
        return rotateInstanceSecret({ body, client, path: { name } })
      case 'project':
        return rotateProjectSecret({
          body,
          client,
          path: { project_id: target.projectId, name },
        })
      case 'pipeline':
        return rotatePipelineSecret({
          body,
          client,
          path: { pipeline_id: target.pipelineId, name },
        })
    }
  })
}

export function useDeleteSecret(target: SecretTarget) {
  return useSecretMutation((client, name: string) => {
    switch (target.scope) {
      case 'instance':
        return deleteInstanceSecret({ client, path: { name } })
      case 'project':
        return deleteProjectSecret({
          client,
          path: { project_id: target.projectId, name },
        })
      case 'pipeline':
        return deletePipelineSecret({
          client,
          path: { pipeline_id: target.pipelineId, name },
        })
    }
  })
}
