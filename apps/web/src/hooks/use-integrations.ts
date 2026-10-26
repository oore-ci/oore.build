import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  checkGitlabPersonalToken as checkGitLabToken,
  deleteIntegration,
  gitlabAuthorize,
  gitlabStart,
  replaceGitlabPersonalToken as replaceGitLabToken,
  rotateGitlabRepositoryWebhookSecret as rotateGitLabRepositoryWebhookSecret,
  syncInstallations,
} from '@oore/client/operations'
import {
  browseLocalGitDirectoriesOptions,
  checkGitlabPersonalTokenMutationKey,
  getIntegrationOptions,
  getIntegrationQueryKey,
  listInstallationsOptions,
  listInstallationsQueryKey,
  listIntegrationsOptions,
  listIntegrationsQueryKey,
  listRepositoriesOptions,
  listRepositoriesQueryKey,
} from '@oore/client/react-query'
import type {
  GitLabAuthorizeRequest,
  GitLabStartRequest,
  ListIntegrationsData,
  ListIntegrationsResponse,
  ListRepositoriesData,
  ReplaceGitLabTokenRequest,
} from '@oore/client/models'
import { useApiContext } from '@/hooks/use-api-context'
import {
  scopeOoreQueryKey,
  scopeOoreQueryOptions,
} from '@/lib/api-client/client'

type ListIntegrationsParams = NonNullable<ListIntegrationsData['query']>
type ListRepositoriesParams = NonNullable<ListRepositoriesData['query']>

export function useIntegrations<TData = ListIntegrationsResponse>(
  params?: ListIntegrationsParams,
  options?: { select?: (data: ListIntegrationsResponse) => TData },
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listIntegrationsOptions({ client, query: params }),
  )

  return useQuery<ListIntegrationsResponse, Error, TData>({
    ...query,
    enabled: !!baseUrl && !!token,
    select: options?.select,
  })
}

export function useIntegration(id: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    getIntegrationOptions({ client, path: { id } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!id,
  })
}

export function useInstallations(integrationId: string) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listInstallationsOptions({ client, path: { id: integrationId } }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useIntegrationRepos(
  integrationId: string,
  params?: ListRepositoriesParams,
) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    listRepositoriesOptions({
      client,
      path: { id: integrationId },
      query: params,
    }),
  )

  return useQuery({
    ...query,
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useSyncInstallations() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (integrationId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return syncInstallations({
        body: {},
        client,
        path: { id: integrationId },
      })
    },
    onSuccess: (_data, integrationId) => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getIntegrationQueryKey({ client, path: { id: integrationId } }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listInstallationsQueryKey({
            client,
            path: { id: integrationId },
          }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listRepositoriesQueryKey({
            client,
            path: { id: integrationId },
          }),
        ),
      })
    },
  })
}

export function useGitLabAuthorize() {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: (data: GitLabAuthorizeRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return gitlabAuthorize({ body: data, client })
    },
    onSuccess: (data) => {
      window.location.href = data.authorize_url
    },
  })
}

export function useGitLabTokenStatus(integrationId: string, enabled: boolean) {
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useQuery({
    queryKey: scopeOoreQueryKey(
      instanceId,
      checkGitlabPersonalTokenMutationKey({
        client,
        path: { id: integrationId },
      }),
    ),
    queryFn: ({ signal }) =>
      checkGitLabToken({ client, path: { id: integrationId }, signal }),
    enabled: enabled && !!baseUrl && !!token && !!integrationId,
    retry: false,
  })
}

export function useReplaceGitLabToken(integrationId: string) {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()
  const statusQueryKey = scopeOoreQueryKey(
    instanceId,
    checkGitlabPersonalTokenMutationKey({
      client,
      path: { id: integrationId },
    }),
  )

  return useMutation({
    mutationFn: (data: ReplaceGitLabTokenRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return replaceGitLabToken({
        body: data,
        client,
        path: { id: integrationId },
      })
    },
    onSuccess: (status) => {
      queryClient.setQueryData(statusQueryKey, status)
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          getIntegrationQueryKey({ client, path: { id: integrationId } }),
        ),
      })
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listIntegrationsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useGitLabStart() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (data: GitLabStartRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return gitlabStart({ body: data, client })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listIntegrationsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useRotateGitLabRepositoryWebhookSecret() {
  const { baseUrl, client, token } = useApiContext()

  return useMutation({
    mutationFn: (repositoryId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return rotateGitLabRepositoryWebhookSecret({
        client,
        path: { id: repositoryId },
      })
    },
  })
}

export function useDeleteIntegration() {
  const queryClient = useQueryClient()
  const { baseUrl, client, instanceId, token } = useApiContext()

  return useMutation({
    mutationFn: (id: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deleteIntegration({ client, path: { id } })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: scopeOoreQueryKey(
          instanceId,
          listIntegrationsQueryKey({ client }),
        ),
      })
    },
  })
}

export function useBrowseLocalGitDirectories(path?: string, enabled = true) {
  const { baseUrl, client, instanceId, token } = useApiContext()
  const query = scopeOoreQueryOptions(
    instanceId,
    browseLocalGitDirectoriesOptions({ client, query: { path } }),
  )

  return useQuery({
    ...query,
    enabled: enabled && !!baseUrl && !!token,
  })
}
