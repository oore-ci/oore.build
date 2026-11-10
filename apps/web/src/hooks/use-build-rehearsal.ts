import { useQuery } from '@tanstack/react-query'
import { getBuildRehearsalOptions } from '@oore/client/react-query'

import { useApiContext } from '@/hooks/use-api-context'
import { scopeOoreQueryOptions } from '@/lib/api-client/client'

/** A rehearsal build's findings, refreshed while it runs. */
export function useBuildRehearsal(buildId: string, running: boolean) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  return useQuery({
    ...scopeOoreQueryOptions(
      instanceId,
      getBuildRehearsalOptions({ client, path: { build_id: buildId } }),
    ),
    enabled: !!baseUrl && !!token && !!buildId,
    refetchInterval: running ? 10_000 : false,
  })
}
