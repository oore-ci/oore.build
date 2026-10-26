import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type { CreateBuildFeedbackRequest } from '@oore/client/models'
import { createBuildFeedback } from '@oore/client/operations'
import {
  getBuildFeedbackScreenshotOptions,
  listBuildFeedbackOptions,
  listBuildFeedbackQueryKey,
} from '@oore/client/react-query'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

/** Testers' notes about a build. Polls so new notes show within a minute. */
export function useBuildFeedback(
  buildId: string,
  options?: { enabled?: boolean },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listBuildFeedbackOptions({ client, path: { build_id: buildId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!buildId && options?.enabled !== false,
    staleTime: 10_000,
    refetchInterval: 30_000,
  })
}

export function useCreateBuildFeedback(buildId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (body: CreateBuildFeedbackRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return createBuildFeedback({ client, path: { build_id: buildId }, body })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listBuildFeedbackQueryKey({ client, path: { build_id: buildId } }),
        ),
      })
    },
  })
}

/** The screenshot attached to a note, fetched with the session token. */
export function useBuildFeedbackScreenshot(
  buildId: string,
  feedbackId: string,
  enabled: boolean,
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getBuildFeedbackScreenshotOptions({
      client,
      path: { build_id: buildId, feedback_id: feedbackId },
    }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && enabled,
    staleTime: Infinity,
  })
}
