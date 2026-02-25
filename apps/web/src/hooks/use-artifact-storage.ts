import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import type {
  ConfigureExternalAccessOidcRequest,
  UpdateArtifactStorageSettingsRequest,
  UpdateExternalAccessNetworkSettingsRequest,
  UpdateInstancePreferencesRequest,
} from '@/lib/types'
import {
  configureExternalAccessOidc,
  getArtifactStorageSettings,
  getExternalAccessNetworkSettings,
  getExternalAccessPreflight,
  getInstancePreferences,
  updateArtifactStorageSettings,
  updateExternalAccessNetworkSettings,
  updateInstancePreferences,
} from '@/lib/api'
import {
  useAuthToken,
  useBaseUrl,
  useInstanceQueryPrefix,
} from '@/hooks/query-context'

export function useArtifactStorageSettings() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'artifact-storage-settings'],
    queryFn: () => getArtifactStorageSettings(baseUrl()!, token()!),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useUpdateArtifactStorageSettings() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: UpdateArtifactStorageSettingsRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return updateArtifactStorageSettings(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'artifact-storage-settings'],
      })
    },
  }))
}

export function useInstancePreferences() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'instance-preferences'],
    queryFn: () => getInstancePreferences(baseUrl()!, token()!),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useUpdateInstancePreferences() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: UpdateInstancePreferencesRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return updateInstancePreferences(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'instance-preferences'],
      })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'external-access-preflight'],
      })
    },
  }))
}

export function useConfigureExternalAccessOidc() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: ConfigureExternalAccessOidcRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return configureExternalAccessOidc(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'external-access-preflight'],
      })
    },
  }))
}

export function useExternalAccessPreflight() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'external-access-preflight'],
    queryFn: () => getExternalAccessPreflight(baseUrl()!, token()!),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useExternalAccessNetworkSettings() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'external-access-network-settings'],
    queryFn: () => getExternalAccessNetworkSettings(baseUrl()!, token()!),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useUpdateExternalAccessNetworkSettings() {
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: UpdateExternalAccessNetworkSettingsRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return updateExternalAccessNetworkSettings(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'external-access-network-settings'],
      })
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'external-access-preflight'],
      })
    },
  }))
}
