import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'

import { getRepositoryAvatar } from '@/lib/api'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

export function useRepositoryAvatarUrl(
  repositoryId: string,
  enabled: boolean,
): string | undefined {
  const instance = useActiveInstance()
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const token = useAuthStore((state) => state.token)
  const expiresAt = useAuthStore((state) => state.expiresAt)
  const authenticated =
    !!token && expiresAt != null && expiresAt > Math.floor(Date.now() / 1000)
  const { data } = useQuery({
    queryKey: [instance?.id ?? '__none__', 'repository-avatar', repositoryId],
    queryFn: ({ signal }) =>
      getRepositoryAvatar(baseUrl!, token!, repositoryId, { signal }),
    enabled: enabled && !!baseUrl && authenticated,
    staleTime: 60 * 60 * 1000,
  })
  const [objectUrl, setObjectUrl] = useState<string>()

  useEffect(() => {
    if (!data) return
    const nextUrl = URL.createObjectURL(data)
    setObjectUrl(nextUrl)
    return () => URL.revokeObjectURL(nextUrl)
  }, [data])

  return objectUrl
}
