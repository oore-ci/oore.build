import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as z from 'zod'

import type {
  AppStorePublishConfig,
  GooglePlayPublishConfig,
} from '@oore/client/models'
import {
  updatePipelineAppStorePublish,
  updatePipelineGooglePlayPublish,
} from '@oore/client/operations'
import {
  getPipelineAppStorePublishOptions,
  getPipelineAppStorePublishQueryKey,
  getPipelineGooglePlayPublishOptions,
  getPipelineGooglePlayPublishQueryKey,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

export type {
  AppStorePublishConfig,
  GooglePlayPublishConfig,
  PipelineAppStorePublishResponse as AppStorePublishResponse,
  PipelineGooglePlayPublishResponse as GooglePlayPublishResponse,
} from '@oore/client/models'

export function usePipelineGooglePlayPublish(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getPipelineGooglePlayPublishOptions({
        client,
        path: { pipeline_id: pipelineId },
      }),
    ),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
  })
}

export function useUpdatePipelineGooglePlayPublish(pipelineId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: async (config: GooglePlayPublishConfig) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return updatePipelineGooglePlayPublish({
        body: config,
        client,
        path: { pipeline_id: pipelineId },
      })
    },
    onSuccess: (response) => {
      queryClient.setQueryData(
        scopeOoreQueryKey(
          instanceId,
          getPipelineGooglePlayPublishQueryKey({
            client,
            path: { pipeline_id: pipelineId },
          }),
        ),
        response,
      )
    },
  })
}

/** Not yet in the generated client. */
const googlePlayCredentialsTestSchema = z.object({
  ok: z.boolean(),
  message: z.string(),
  package_name: z.string().nullish(),
  service_account_email: z.string().nullish(),
  checked_at: z.number(),
})

export type GooglePlayCredentialsTest = z.infer<
  typeof googlePlayCredentialsTestSchema
>

/**
 * Checks a service account key against Google Play by opening and discarding
 * an edit for the app. A rejected key is a result with `ok: false`, not an
 * error.
 */
export function useTestGooglePlayCredentials(pipelineId: string) {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: async (body: {
      service_account_secret: string
      package_name: string | null
    }) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      const response = await client.post({
        body,
        headers: { 'Content-Type': 'application/json' },
        responseStyle: 'data',
        security: [{ scheme: 'bearer', type: 'http' }],
        throwOnError: true,
        url: `/v1/pipelines/${encodeURIComponent(pipelineId)}/publish/google-play/test-credentials`,
      })
      return googlePlayCredentialsTestSchema.parse(response)
    },
  })
}

export function usePipelineAppStorePublish(
  pipelineId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getPipelineAppStorePublishOptions({
        client,
        path: { pipeline_id: pipelineId },
      }),
    ),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
  })
}

export function useUpdatePipelineAppStorePublish(pipelineId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: async (config: AppStorePublishConfig) => {
      if (!baseUrl || !token) throw new Error('Not authenticated')
      return updatePipelineAppStorePublish({
        body: config,
        client,
        path: { pipeline_id: pipelineId },
      })
    },
    onSuccess: (response) => {
      queryClient.setQueryData(
        scopeOoreQueryKey(
          instanceId,
          getPipelineAppStorePublishQueryKey({
            client,
            path: { pipeline_id: pipelineId },
          }),
        ),
        response,
      )
    },
  })
}
