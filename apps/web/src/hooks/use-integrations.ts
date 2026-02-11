import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  GitHubAppStartRequest,
  GitLabAuthorizeRequest,
  GitLabStartRequest,
} from '@/lib/types'
import {
  deleteIntegration,
  getIntegration,
  githubAppComplete,
  githubAppStart,
  gitlabAuthorize,
  gitlabStart,
  listInstallations,
  listIntegrationRepos,
  listIntegrations,
  syncInstallations,
} from '@/lib/api'
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

export function useIntegrations(provider?: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integrations', provider ?? 'all'],
    queryFn: () => listIntegrations(baseUrl!, token!, { provider }),
    enabled: !!baseUrl && !!token,
  })
}

export function useIntegration(id: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integration', id],
    queryFn: () => getIntegration(baseUrl!, token!, id),
    enabled: !!baseUrl && !!token && !!id,
  })
}

export function useInstallations(integrationId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'installations', integrationId],
    queryFn: () => listInstallations(baseUrl!, token!, integrationId),
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useIntegrationRepos(integrationId: string) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'integration-repos', integrationId],
    queryFn: () => listIntegrationRepos(baseUrl!, token!, integrationId),
    enabled: !!baseUrl && !!token && !!integrationId,
  })
}

export function useGitHubAppStart() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return useMutation({
    mutationFn: (data: GitHubAppStartRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return githubAppStart(baseUrl, token, data)
    },
  })
}

export function useGitHubAppComplete() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (code: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return githubAppComplete(baseUrl, token, { code })
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'integrations'],
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
