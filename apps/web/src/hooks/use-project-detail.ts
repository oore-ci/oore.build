import { useQuery } from '@tanstack/react-query'

import { getProject } from '@/lib/api'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

function useAuthToken(): string | null {
  const token = useAuthStore((state) => state.token)
  const expiresAt = useAuthStore((state) => state.expiresAt)
  if (!token || expiresAt == null) return null
  if (expiresAt <= Math.floor(Date.now() / 1000)) return null
  return token
}

export function useProjectDetail(projectId: string) {
  const instance = useActiveInstance()
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const token = useAuthToken()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'project', projectId],
    queryFn: ({ signal }) =>
      getProject(baseUrl!, token!, projectId, { signal }),
    enabled: !!baseUrl && !!token && !!projectId,
  })
}
