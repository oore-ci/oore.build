import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { GitLabAuthorizeRequest, GitLabStartRequest } from '@/lib/types'
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
  updateRepositoryRunnerPolicy,
} from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
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
  return resolveInstanceApiBaseUrl(instance)
}

export function useIntegrations(provider?: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integrations', provider ?? 'all'],
    queryFn: ({ signal }) =>
      listAllIntegrations(baseUrl!, token!, provider, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useIntegration(id: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integration', id],
    queryFn: ({ signal }) => getIntegration(baseUrl!, token!, id, { signal }),
    enabled: !!baseUrl && !!token && !!id,
  })
}

export function useInstallations(integrationId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'installations', integrationId],
    queryFn: ({ signal }) =>
      listInstallations(baseUrl!, token!, integrationId, { signal }),
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useIntegrationRepos(integrationId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integration-repos', integrationId],
    queryFn: ({ signal }) =>
      listIntegrationRepos(baseUrl!, token!, integrationId, { signal }),
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useUpdateRepositoryRunnerPolicy(integrationId: string) {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      repositoryId,
      allow,
    }: {
      repositoryId: string
      allow: boolean
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updateRepositoryRunnerPolicy(baseUrl, token, repositoryId, {
        allow_direct_macos_runner: allow,
      })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'integration-repos',
          integrationId,
        ],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'all-repos-for-project'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'all-repos-for-runner-policy'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'builds'],
      })
    },
  })
}

export function useSyncInstallations() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'all-repos-for-runner-policy'],
      })
    },
  })
}

export function useGitLabAuthorize() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

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
