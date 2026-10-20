import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { GooglePlayPublishConfig } from '@oore/client/models'
import { updatePipelineGooglePlayPublish } from '@oore/client/operations'
import {
  getPipelineGooglePlayPublishOptions,
  getPipelineGooglePlayPublishQueryKey,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

export type {
  GooglePlayPublishConfig,
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
