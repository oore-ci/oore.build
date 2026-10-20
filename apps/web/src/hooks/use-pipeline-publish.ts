import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as z from 'zod'

import { useApiContext } from '@/hooks/use-api-context'
import {
  requestOoreOperation,
  scopeOoreQueryKey,
} from '@/lib/api-client/client'

// TODO(oore-client-js): switch to the generated publish operations once the
// client is regenerated from an OpenAPI that includes #474.
const googlePlayPublishConfigSchema = z.object({
  enabled: z.boolean(),
  service_account_secret: z.string(),
  package_name: z.string().nullish(),
  track: z.string(),
  rollout_percent: z.number().nullish(),
  draft: z.boolean(),
  release_notes: z.enum(['none', 'changelog', 'repo_files']),
  release_notes_locale: z.string(),
  release_notes_dir: z.string().nullish(),
  upload_mapping: z.boolean(),
  in_app_update_priority: z.number().int().nullish(),
})

const googlePlayPublishResponseSchema = z.object({
  pipeline_id: z.string(),
  config: googlePlayPublishConfigSchema.nullish(),
  service_account_secret_present: z.boolean(),
  updated_at: z.number().nullish(),
})

export type GooglePlayPublishConfig = z.infer<
  typeof googlePlayPublishConfigSchema
>
export type GooglePlayPublishResponse = z.infer<
  typeof googlePlayPublishResponseSchema
>

function googlePlayUrl(pipelineId: string) {
  return `/v1/pipelines/${encodeURIComponent(pipelineId)}/publish/google-play`
}

function googlePlayQueryKey(instanceId: string, pipelineId: string) {
  return scopeOoreQueryKey(instanceId, [
    'pipeline-publish',
    pipelineId,
    'google-play',
  ])
}

export function usePipelineGooglePlayPublish(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    queryKey: googlePlayQueryKey(instanceId, pipelineId),
    queryFn: async ({ signal }) =>
      requestOoreOperation({
        client,
        method: 'GET',
        schema: googlePlayPublishResponseSchema,
        signal,
        url: googlePlayUrl(pipelineId),
      }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
  })
}

export function useUpdatePipelineGooglePlayPublish(pipelineId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: async (config: GooglePlayPublishConfig) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return requestOoreOperation({
        body: config,
        client,
        method: 'PUT',
        schema: googlePlayPublishResponseSchema,
        url: googlePlayUrl(pipelineId),
      })
    },
    onSuccess: (response) => {
      queryClient.setQueryData(
        googlePlayQueryKey(instanceId, pipelineId),
        response,
      )
    },
  })
}
