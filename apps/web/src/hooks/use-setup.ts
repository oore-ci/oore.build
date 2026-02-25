import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import type {
  OidcConfigureRequest,
  SetupPreferencesRequest,
  SetupTrustedProxyConfigureRequest,
} from '@/lib/types'
import {
  completeSetup,
  configureOidc,
  getSetupStatus,
  getSetupSummary,
  setupLocalOwnerCreate,
  setupOidcStart,
  setupOidcVerify,
  setupPreferences,
  setupTrustedProxyClaimOwner,
  setupTrustedProxyConfigure,
  verifyBootstrapToken,
} from '@/lib/api'
import { useBaseUrl, useInstanceQueryPrefix } from '@/hooks/query-context'
import { useSetupStore } from '@/stores/setup-store'

export function useSetupStatus() {
  const baseUrl = useBaseUrl()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'setup-status'],
    queryFn: () => getSetupStatus(baseUrl()!),
    enabled: !!baseUrl(),
    staleTime: 1_000,
    retry: 1,
  }))
}

export function useVerifyBootstrapToken() {
  const baseUrl = useBaseUrl()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (token: string) => {
      if (!baseUrl()) throw new Error('Missing instance URL')
      const response = await verifyBootstrapToken(baseUrl()!, token)
      useSetupStore.getState().setSessionToken(response.session_token)
      useSetupStore.getState().setSessionExpiresAt(response.expires_at)
      return response
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'setup-status'],
      })
    },
  }))
}

export function useConfigureOidc() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)

  return createMutation(() => ({
    mutationFn: async (data: OidcConfigureRequest) => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      const response = await configureOidc(baseUrl()!, sessionToken()!, data)
      if (response.session_expires_at != null) {
        useSetupStore.getState().setSessionExpiresAt(response.session_expires_at)
      }
      return response
    },
  }))
}

export function useSetupPreferences() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)

  return createMutation(() => ({
    mutationFn: async (data: SetupPreferencesRequest) => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      const response = await setupPreferences(baseUrl()!, sessionToken()!, data)
      if (response.session_expires_at != null) {
        useSetupStore.getState().setSessionExpiresAt(response.session_expires_at)
      }
      return response
    },
  }))
}

export function useSetupOidcStart() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)

  return createMutation(() => ({
    mutationFn: async (redirectUri: string) => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      return setupOidcStart(baseUrl()!, sessionToken()!, redirectUri)
    },
  }))
}

export function useSetupOidcVerify() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)

  return createMutation(() => ({
    mutationFn: async ({ code, state }: { code: string; state: string }) => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      const response = await setupOidcVerify(
        baseUrl()!,
        sessionToken()!,
        code,
        state,
      )
      if (response.session_expires_at != null) {
        useSetupStore.getState().setSessionExpiresAt(response.session_expires_at)
      }
      return response
    },
  }))
}

export function useSetupLocalOwnerCreate() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)

  return createMutation(() => ({
    mutationFn: async (email: string) => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      const response = await setupLocalOwnerCreate(
        baseUrl()!,
        sessionToken()!,
        email,
      )
      if (response.session_expires_at != null) {
        useSetupStore.getState().setSessionExpiresAt(response.session_expires_at)
      }
      return response
    },
  }))
}

export function useSetupTrustedProxyConfigure() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)

  return createMutation(() => ({
    mutationFn: async (data: SetupTrustedProxyConfigureRequest) => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      const response = await setupTrustedProxyConfigure(
        baseUrl()!,
        sessionToken()!,
        data,
      )
      if (response.session_expires_at != null) {
        useSetupStore.getState().setSessionExpiresAt(response.session_expires_at)
      }
      return response
    },
  }))
}

export function useSetupTrustedProxyClaimOwner() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)

  return createMutation(() => ({
    mutationFn: async () => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      const response = await setupTrustedProxyClaimOwner(
        baseUrl()!,
        sessionToken()!,
      )
      if (response.session_expires_at != null) {
        useSetupStore.getState().setSessionExpiresAt(response.session_expires_at)
      }
      return response
    },
  }))
}

export function useSetupSummary() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'setup-summary'],
    queryFn: () => getSetupSummary(baseUrl()!, sessionToken()!),
    enabled: !!baseUrl() && !!sessionToken(),
  }))
}

export function useCompleteSetup() {
  const baseUrl = useBaseUrl()
  const sessionToken = useSetupStore((state) => state.sessionToken)
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async () => {
      if (!baseUrl() || !sessionToken()) throw new Error('Missing setup session')
      return completeSetup(baseUrl()!, sessionToken()!)
    },
    onSuccess: () => {
      useSetupStore.getState().reset()
      void queryClient.invalidateQueries({
        queryKey: [prefix(), 'setup-status'],
      })
    },
  }))
}
