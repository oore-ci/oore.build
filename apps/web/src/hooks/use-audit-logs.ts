import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { listAuditLogs } from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

function useAuthToken(): string | null {
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  if (!token || expiresAt == null) return null
  if (expiresAt <= Math.floor(Date.now() / 1000)) return null
  return token
}

function useBaseUrl(): string | null {
  const instance = useActiveInstance()
  return instance?.url ?? null
}

export function useAuditLogs(params?: {
  limit?: number
  offset?: number
  actor_id?: string
  action?: string
  resource_type?: string
  from_ts?: number
  to_ts?: number
}) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'audit-logs', params ?? {}],
    queryFn: () => listAuditLogs(baseUrl!, token!, params),
    enabled: baseUrl !== null && !!token,
    placeholderData: keepPreviousData,
  })
}
