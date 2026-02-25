import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import type {
  CreateLocalGitIntegrationRequest,
  GitHubAppStartRequest,
  GitLabAuthorizeRequest,
  GitLabStartRequest,
} from '@/lib/types'
import {
  browseLocalGitDirectories,
  createLocalGitIntegration,
  deleteIntegration,
  deleteLocalGitIntegration,
  getIntegration,
  githubAppComplete,
  githubAppStart,
  gitlabAuthorize,
  gitlabStart,
  listInstallations,
  listIntegrationRepos,
  listIntegrations,
  listLocalGitIntegrations,
  syncInstallations,
} from '@/lib/api'
import {
  useAuthToken,
  useBaseUrl,
  useInstanceQueryPrefix,
} from '@/hooks/query-context'

export function useIntegrations(provider?: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'integrations', provider ?? 'all'],
    queryFn: () => listIntegrations(baseUrl()!, token()!, { provider }),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useIntegration(id: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'integration', id],
    queryFn: () => getIntegration(baseUrl()!, token()!, id),
    enabled: !!baseUrl() && !!token() && !!id,
  }))
}

export function useInstallations(integrationId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'installations', integrationId],
    queryFn: () => listInstallations(baseUrl()!, token()!, integrationId),
    enabled: !!baseUrl() && !!token() && !!integrationId,
  }))
}

export function useIntegrationRepos(integrationId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'integration-repos', integrationId],
    queryFn: () => listIntegrationRepos(baseUrl()!, token()!, integrationId),
    enabled: !!baseUrl() && !!token() && !!integrationId,
  }))
}

export function useRepositoryProvider(repositoryId?: string, enabled = true) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'repository-provider', repositoryId ?? '__none__'],
    queryFn: async () => {
      if (!baseUrl() || !token() || !repositoryId) return null
      const integrations = await listIntegrations(baseUrl()!, token()!)

      for (const integration of integrations.integrations) {
        try {
          const repos = await listIntegrationRepos(
            baseUrl()!,
            token()!,
            integration.id,
          )
          if (repos.repositories.some((repo) => repo.id === repositoryId)) {
            return integration.provider
          }
        } catch {
          // Skip integrations that fail to enumerate repositories.
        }
      }

      return null
    },
    enabled: enabled && !!baseUrl() && !!token() && !!repositoryId,
  }))
}

export function useGitHubAppStart() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return createMutation(() => ({
    mutationFn: async (data: GitHubAppStartRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return githubAppStart(baseUrl()!, token()!, data)
    },
  }))
}

export function useGitHubAppComplete() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (code: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return githubAppComplete(baseUrl()!, token()!, { code })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integrations'],
      })
    },
  }))
}

export function useSyncInstallations() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (integrationId: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return syncInstallations(baseUrl()!, token()!, integrationId)
    },
    onSuccess: (_data, integrationId) => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integration', integrationId],
      })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'installations', integrationId],
      })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integration-repos', integrationId],
      })
    },
  }))
}

export function useGitLabAuthorize() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return createMutation(() => ({
    mutationFn: async (data: GitLabAuthorizeRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return gitlabAuthorize(baseUrl()!, token()!, data)
    },
    onSuccess: (data) => {
      window.location.href = data.authorize_url
    },
  }))
}

export function useGitLabStart() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: GitLabStartRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return gitlabStart(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integrations'],
      })
    },
  }))
}

export function useDeleteIntegration() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (id: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return deleteIntegration(baseUrl()!, token()!, id)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integrations'],
      })
    },
  }))
}

export function useLocalGitIntegrations(enabled = true) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'integrations', 'local-git'],
    queryFn: () => listLocalGitIntegrations(baseUrl()!, token()!),
    enabled: enabled && !!baseUrl() && !!token(),
  }))
}

export function useCreateLocalGitIntegration() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: CreateLocalGitIntegrationRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return createLocalGitIntegration(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integrations'],
      })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integrations', 'local-git'],
      })
    },
  }))
}

export function useBrowseLocalGitDirectories(path?: string, enabled = true) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'local-git-directory-browser', path ?? '__default__'],
    queryFn: () => browseLocalGitDirectories(baseUrl()!, token()!, path),
    enabled: enabled && !!baseUrl() && !!token(),
  }))
}

export function useDeleteLocalGitIntegration() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (id: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return deleteLocalGitIntegration(baseUrl()!, token()!, id)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integrations'],
      })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'integrations', 'local-git'],
      })
    },
  }))
}
