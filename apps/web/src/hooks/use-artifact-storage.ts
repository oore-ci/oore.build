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
import { useApiContext } from '@/hooks/use-api-context'

export function useArtifactStorageSettings() {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'artifact-storage-settings'],
    queryFn: ({ signal }) =>
      getArtifactStorageSettings(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
    select: (response) => response.settings,
  })
}

export function useUpdateArtifactStorageSettings() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'instance-preferences'],
    queryFn: ({ signal }) =>
      getInstancePreferences(baseUrl!, token!, { signal }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
    select: (response) => response.preferences,
  })
}

export function useUpdateInstancePreferences() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'external-access-oidc'],
    queryFn: ({ signal }) =>
      getExternalAccessOidc(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useConfigureExternalAccessOidc() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
  const { baseUrl, token } = useApiContext()

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
  const { baseUrl, instance, token } = useApiContext()

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
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'external-access-network-settings'],
    queryFn: ({ signal }) =>
      getExternalAccessNetworkSettings(baseUrl!, token!, { signal }),
    enabled: (options?.enabled ?? true) && !!baseUrl && !!token,
    select: (response) => response.settings,
  })
}

export function useUpdateExternalAccessNetworkSettings() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'external-access-trusted-proxy'],
    queryFn: ({ signal }) =>
      getExternalAccessTrustedProxySettings(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
    select: (response) => response.settings,
  })
}

export function useUpdateExternalAccessTrustedProxySettings() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

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
