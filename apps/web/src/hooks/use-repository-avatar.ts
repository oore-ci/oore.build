import { useQuery } from '@tanstack/react-query'

import { getRepositoryAvatar } from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

export function useRepositoryAvatar(repositoryId: string) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'repository-avatar', repositoryId],
    queryFn: ({ signal }) =>
      getRepositoryAvatar(baseUrl!, token!, repositoryId, { signal }),
    enabled: !!baseUrl && !!token,
    staleTime: 60 * 60 * 1000,
  })
}
