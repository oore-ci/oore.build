import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import type {
  ConfigureExternalAccessOidcRequest,
  TestOidcConnectionRequest,
  UpdateArtifactStorageSettingsRequest,
  UpdateExternalAccessNetworkSettingsRequest,
  UpdateInstancePreferencesRequest,
  UpdateTrustedProxySettingsRequest,
} from '@/lib/types'
import {
  configureExternalAccessOidc,
  getArtifactStorageSettings,
  getExternalAccessNetworkSettings,
  getExternalAccessOidc,
  getExternalAccessPreflight,
  getExternalAccessTrustedProxySettings,
  getInstancePreferences,
  testOidcConnection,
  updateArtifactStorageSettings,
  updateExternalAccessNetworkSettings,
  updateExternalAccessTrustedProxySettings,
  updateInstancePreferences,
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

export function useArtifactStorageSettings() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'artifact-storage-settings'],
    queryFn: ({ signal }) =>
      getArtifactStorageSettings(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useUpdateArtifactStorageSettings() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: UpdateArtifactStorageSettingsRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateArtifactStorageSettings(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'artifact-storage-settings'],
      })
    },
  })
}

export function useInstancePreferences(options?: { enabled?: boolean }) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'instance-preferences'],
    queryFn: ({ signal }) =>
      getInstancePreferences(baseUrl!, token!, { signal }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
  })
}

export function useUpdateInstancePreferences() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: UpdateInstancePreferencesRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateInstancePreferences(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'instance-preferences'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'external-access-preflight'],
      })
    },
  })
}

export function useExternalAccessOidc() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'external-access-oidc'],
    queryFn: ({ signal }) =>
      getExternalAccessOidc(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useConfigureExternalAccessOidc() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: ConfigureExternalAccessOidcRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return configureExternalAccessOidc(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'external-access-preflight'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'external-access-oidc'],
      })
    },
  })
}

export function useTestOidcConnection() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()

  return useMutation({
    mutationFn: (data: TestOidcConnectionRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return testOidcConnection(baseUrl, token, data)
    },
  })
}

export function useExternalAccessPreflight() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'external-access-preflight'],
    queryFn: ({ signal }) =>
      getExternalAccessPreflight(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useExternalAccessNetworkSettings(options?: {
  enabled?: boolean
}) {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'external-access-network-settings'],
    queryFn: ({ signal }) =>
      getExternalAccessNetworkSettings(baseUrl!, token!, { signal }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
  })
}

export function useUpdateExternalAccessNetworkSettings() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: UpdateExternalAccessNetworkSettingsRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateExternalAccessNetworkSettings(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [
          instance?.id ?? '__none__',
          'external-access-network-settings',
        ],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'external-access-preflight'],
      })
    },
  })
}

export function useExternalAccessTrustedProxySettings() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'external-access-trusted-proxy'],
    queryFn: ({ signal }) =>
      getExternalAccessTrustedProxySettings(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useUpdateExternalAccessTrustedProxySettings() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: (data: UpdateTrustedProxySettingsRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return updateExternalAccessTrustedProxySettings(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'external-access-trusted-proxy'],
      })
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'external-access-preflight'],
      })
    },
  })
}
