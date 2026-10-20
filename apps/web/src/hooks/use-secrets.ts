import {
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import * as z from 'zod'

import { useApiContext } from '@/hooks/use-api-context'
import {
  requestOoreOperation,
  scopeOoreQueryKey,
} from '@/lib/api-client/client'

// TODO(oore-client-js): switch to the generated secret operations once the
// client is regenerated from an OpenAPI that includes #467.
const secretSummarySchema = z.object({
  name: z.string(),
  scope: z.enum(['instance', 'project', 'pipeline']),
  project_id: z.string().nullish(),
  pipeline_id: z.string().nullish(),
  created_at: z.number(),
  updated_at: z.number(),
})
const listSecretsSchema = z.object({ secrets: z.array(secretSummarySchema) })
const secretResponseSchema = z.object({ secret: secretSummarySchema })
const okResponseSchema = z.object({ ok: z.boolean() })

export type SecretSummary = z.infer<typeof secretSummarySchema>
export type SecretScope = SecretSummary['scope']

export type SecretTarget =
  | { scope: 'instance' }
  | { scope: 'project'; projectId: string }
  | { scope: 'pipeline'; pipelineId: string }

export const SECRET_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/
export const SECRET_NAME_MAX_LENGTH = 128
export const SECRET_VALUE_MAX_BYTES = 64 * 1024

function secretsUrl(target: SecretTarget) {
  switch (target.scope) {
    case 'instance':
      return '/v1/secrets'
    case 'project':
      return `/v1/projects/${encodeURIComponent(target.projectId)}/secrets`
    case 'pipeline':
      return `/v1/pipelines/${encodeURIComponent(target.pipelineId)}/secrets`
  }
}

function targetId(target: SecretTarget) {
  switch (target.scope) {
    case 'instance':
      return 'instance'
    case 'project':
      return target.projectId
    case 'pipeline':
      return target.pipelineId
  }
}

function secretsQueryKey(instanceId: string, target?: SecretTarget) {
  return scopeOoreQueryKey(
    instanceId,
    target ? ['secrets', target.scope, targetId(target)] : ['secrets'],
  )
}

function secretsQueryOptions(
  context: ReturnType<typeof useApiContext>,
  target: SecretTarget,
  enabled: boolean,
) {
  const { baseUrl, client, instanceId, token } = context
  return {
    queryKey: secretsQueryKey(instanceId, target),
    queryFn: async ({ signal }: { signal: AbortSignal }) =>
      (
        await requestOoreOperation({
          client,
          method: 'GET',
          schema: listSecretsSchema,
          signal,
          url: secretsUrl(target),
        })
      ).secrets,
    enabled: enabled && !!baseUrl && !!token,
  }
}

export function useSecrets(
  target: SecretTarget,
  options?: { enabled?: boolean },
) {
  const context = useApiContext()
  return useQuery(
    secretsQueryOptions(context, target, options?.enabled ?? true),
  )
}

/**
 * Secrets from broader scopes, narrowest first. Scopes the caller cannot
 * read are left out rather than failing the whole list.
 */
export function useInheritedSecrets(targets: Array<SecretTarget>) {
  const context = useApiContext()
  return useQueries({
    queries: targets.map((target) =>
      secretsQueryOptions(context, target, true),
    ),
    combine: (results) => results.flatMap((result) => result.data ?? []),
  })
}

function useSecretMutation<TVariables, TResult>(
  request: (variables: TVariables) => {
    body?: unknown
    method: 'DELETE' | 'POST' | 'PUT'
    schema: z.ZodType<TResult>
    url: string
  },
) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: async (variables: TVariables) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return requestOoreOperation({ client, ...request(variables) })
    },
    // Every scope's list can change what a pipeline resolves, and publish
    // settings report whether their referenced secret exists.
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: secretsQueryKey(instanceId),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(instanceId, ['pipeline-publish']),
      })
    },
  })
}

export function useCreateSecret(target: SecretTarget) {
  return useSecretMutation(
    ({ name, value }: { name: string; value: string }) => ({
      body: { name, value },
      method: 'POST',
      schema: secretResponseSchema,
      url: secretsUrl(target),
    }),
  )
}

export function useRotateSecret(target: SecretTarget) {
  return useSecretMutation(
    ({ name, value }: { name: string; value: string }) => ({
      body: { value },
      method: 'PUT',
      schema: secretResponseSchema,
      url: `${secretsUrl(target)}/${encodeURIComponent(name)}`,
    }),
  )
}

export function useDeleteSecret(target: SecretTarget) {
  return useSecretMutation((name: string) => ({
    method: 'DELETE',
    schema: okResponseSchema,
    url: `${secretsUrl(target)}/${encodeURIComponent(name)}`,
  }))
}
