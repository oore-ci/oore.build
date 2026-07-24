import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  GitLabAuthorizeRequest,
  GitLabStartRequest,
  ListIntegrationsResponse,
} from '@/lib/types'
import {
  browseLocalGitDirectories,
  deleteIntegration,
  getIntegration,
  gitlabAuthorize,
  gitlabStart,
  listAllIntegrations,
  listInstallations,
  listIntegrationRepos,
  rotateGitLabRepositoryWebhookSecret,
  syncInstallations,
} from '@/lib/api'
import { useApiContext } from '@/hooks/use-api-context'

export function useIntegrations<TData = ListIntegrationsResponse>(
  provider?: string,
  options?: { select?: (data: ListIntegrationsResponse) => TData },
) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery<ListIntegrationsResponse, Error, TData>({
    queryKey: [instance?.id ?? '__none__', 'integrations', provider ?? 'all'],
    queryFn: ({ signal }) =>
      listAllIntegrations(baseUrl!, token!, provider, { signal }),
    enabled: !!baseUrl && !!token,
    select: options?.select,
  })
}

export function useIntegration(id: string) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integration', id],
    queryFn: ({ signal }) => getIntegration(baseUrl!, token!, id, { signal }),
    enabled: !!baseUrl && !!token && !!id,
  })
}

export function useInstallations(integrationId: string) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'installations', integrationId],
    queryFn: ({ signal }) =>
      listInstallations(baseUrl!, token!, integrationId, { signal }),
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useIntegrationRepos(integrationId: string) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integration-repos', integrationId],
    queryFn: ({ signal }) =>
      listIntegrationRepos(baseUrl!, token!, integrationId, { signal }),
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useSyncInstallations() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (integrationId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return syncInstallations(baseUrl, token, integrationId)
    },
    onSuccess: (_data, integrationId) => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'integration', integrationId],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'installations', integrationId],
      })
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'integration-repos',
          integrationId,
        ],
      })
    },
  })
}

export function useGitLabAuthorize() {
  const { baseUrl, token } = useApiContext()

  return useMutation({
    mutationFn: (data: GitLabAuthorizeRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return gitlabAuthorize(baseUrl, token, data)
    },
    onSuccess: (data) => {
      window.location.href = data.authorize_url
    },
  })
}

export function useGitLabStart() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (data: GitLabStartRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return gitlabStart(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'integrations'],
      })
    },
  })
}

export function useRotateGitLabRepositoryWebhookSecret() {
  const { baseUrl, token } = useApiContext()

  return useMutation({
    mutationFn: (repositoryId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return rotateGitLabRepositoryWebhookSecret(baseUrl, token, repositoryId)
    },
  })
}

export function useDeleteIntegration() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (id: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deleteIntegration(baseUrl, token, id)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'integrations'],
      })
    },
  })
}

export function useBrowseLocalGitDirectories(path?: string, enabled = true) {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [
      instance?.id ?? '__none__',
      'local-git-directory-browser',
      path ?? '__default__',
    ],
    queryFn: ({ signal }) =>
      browseLocalGitDirectories(baseUrl!, token!, path, { signal }),
    enabled: enabled && !!baseUrl && !!token,
  })
}
