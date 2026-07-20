import { useQuery } from '@tanstack/react-query'

import { getRepositoryAvatar } from '@/lib/api'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

const createObjectURL = (blob: Blob) => URL.createObjectURL(blob)

export function useRepositoryAvatar(repositoryId: string) {
  const instance = useActiveInstance()
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const token = useAuthStore(({ token, expiresAt }) =>
    token && expiresAt && expiresAt > Date.now() / 1000 ? token : null,
  )

  const response = useQuery({
    queryKey: [instance?.id ?? '__none__', 'repository-avatar', repositoryId],
    queryFn: ({ signal }) =>
      getRepositoryAvatar(baseUrl!, token!, repositoryId, { signal }),
    select: createObjectURL,
    enabled: !!baseUrl && !!token,
    staleTime: 60 * 60 * 1000,
  })

  return response.data
}
