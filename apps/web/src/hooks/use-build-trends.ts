import { useQuery } from '@tanstack/react-query'
import {
  getBuildFailureHintOptions,
  getBuildTrendsOptions,
} from '@oore/client/react-query'

import { useApiContext } from '@/hooks/use-api-context'
import { scopeOoreQueryOptions } from '@/lib/api-client/client'
import type { BuildInsightsWindow } from '@/hooks/use-builds'

export function useBuildTrends(days: BuildInsightsWindow) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getBuildTrendsOptions({ client, query: { days } }),
    ),
    enabled: !!baseUrl && !!token,
    staleTime: 5 * 60_000,
  })
}

export function useBuildFailureHint(buildId: string, enabled: boolean) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getBuildFailureHintOptions({ client, path: { build_id: buildId } }),
    ),
    enabled: !!baseUrl && !!token && enabled,
    staleTime: Infinity,
  })
}
