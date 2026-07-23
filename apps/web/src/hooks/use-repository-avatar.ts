import { useQuery } from '@tanstack/react-query'

import { getRepositoryAvatar } from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

const createObjectURL = (blob: Blob) => URL.createObjectURL(blob)

export function useRepositoryAvatar(repositoryId: string) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'repository-avatar', repositoryId],
    queryFn: ({ signal }) =>
      getRepositoryAvatar(baseUrl!, token!, repositoryId, { signal }),
    select: createObjectURL,
    enabled: !!baseUrl && !!token,
    staleTime: 60 * 60 * 1000,
  })
}
