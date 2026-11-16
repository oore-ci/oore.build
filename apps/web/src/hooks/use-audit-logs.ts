import { keepPreviousData, useQuery } from '@tanstack/react-query'
import {
  getAuditLogActivityOptions,
  listAuditLogsOptions,
} from '@oore/client/react-query'
import type { AuditLogCategory } from '@oore/client/models'
import { useApiContext } from '@/hooks/use-api-context'
import { scopeOoreQueryOptions } from '@/lib/api-client/client'

export function useAuditLogs(params?: {
  limit?: number
  offset?: number
  actor_id?: string
  token_id?: string
  q?: string
  category?: AuditLogCategory
  from_ts?: number
  to_ts?: number
}) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listAuditLogsOptions({ client, query: params }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token,
    placeholderData: keepPreviousData,
  })
}

/** Entries per day over the last `days`, split at the viewer's midnight. */
export function useAuditLogActivity(days: number) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getAuditLogActivityOptions({
      client,
      query: {
        days,
        utc_offset_minutes: -new Date().getTimezoneOffset(),
      },
    }),
  )

  return useQuery({ ...query, enabled: !!baseUrl && !!token })
}
