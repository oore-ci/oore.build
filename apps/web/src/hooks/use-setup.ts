import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { Instance, OidcConfigureRequest } from '@/lib/types'
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
import { useActiveInstance } from '@/stores/instance-store'
import { useSetupStore } from '@/stores/setup-store'
import { resolveRequiredInstanceApiBaseUrl } from '@/lib/instance-url'

function requireInstance(instance: Instance | null): string {
  return resolveRequiredInstanceApiBaseUrl(instance)
}

function useSetupStatusKey() {
  const instance = useActiveInstance()
  return [instance?.id ?? '__none__', 'setup-status'] as const
}

export function useSetupStatus() {
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useQuery({
    queryKey,
    queryFn: () => getSetupStatus(requireInstance(instance)),
    refetchInterval: 3000,
    enabled: !!instance,
  })
}

export function useVerifyBootstrapToken() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: (token: string) =>
      verifyBootstrapToken(requireInstance(instance), token),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}

export function useConfigureOidc() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: ({
      sessionToken,
      data,
    }: {
      sessionToken: string
      data: OidcConfigureRequest
    }) => configureOidc(requireInstance(instance), sessionToken, data),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}

export function useSetupOidcStart() {
  const instance = useActiveInstance()

  return useMutation({
    mutationFn: ({
      sessionToken,
      redirectUri,
    }: {
      sessionToken: string
      redirectUri: string
    }) => setupOidcStart(requireInstance(instance), sessionToken, redirectUri),
  })
}

export function useSetupOidcVerify() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: ({
      sessionToken,
      code,
      state,
    }: {
      sessionToken: string
      code: string
      state: string
    }) => setupOidcVerify(requireInstance(instance), sessionToken, code, state),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}

export function useSetupLocalOwnerCreate() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: ({
      sessionToken,
      email,
    }: {
      sessionToken: string
      email: string
    }) => setupLocalOwnerCreate(requireInstance(instance), sessionToken, email),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}

export function useSetupPreferences() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: ({
      sessionToken,
      runtimeMode,
      remoteAuthMode,
    }: {
      sessionToken: string
      runtimeMode: 'local' | 'remote'
      remoteAuthMode?: 'oidc' | 'trusted_proxy'
    }) =>
      setupPreferences(requireInstance(instance), sessionToken, {
        runtime_mode: runtimeMode,
        remote_auth_mode: remoteAuthMode,
      }),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}

export function useSetupTrustedProxyConfigure() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: ({
      sessionToken,
      userEmailHeader,
      setupOwnerEmail,
      trustedProxyCidrs,
      sharedSecret,
    }: {
      sessionToken: string
      userEmailHeader?: string
      setupOwnerEmail?: string
      trustedProxyCidrs: Array<string>
      sharedSecret?: string
    }) =>
      setupTrustedProxyConfigure(requireInstance(instance), sessionToken, {
        user_email_header: userEmailHeader,
        setup_owner_email: setupOwnerEmail,
        trusted_proxy_cidrs: trustedProxyCidrs,
        shared_secret: sharedSecret,
      }),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}

export function useSetupTrustedProxyClaimOwner() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: ({ sessionToken }: { sessionToken: string }) =>
      setupTrustedProxyClaimOwner(requireInstance(instance), sessionToken),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}

export function useSetupSummary() {
  const instance = useActiveInstance()
  const sessionToken = useSetupStore((s) => s.sessionToken)

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'setup-summary'] as const,
    queryFn: () => getSetupSummary(requireInstance(instance), sessionToken!),
    enabled: !!instance && !!sessionToken,
  })
}

export function useCompleteSetup() {
  const queryClient = useQueryClient()
  const instance = useActiveInstance()
  const queryKey = useSetupStatusKey()

  return useMutation({
    mutationFn: (sessionToken: string) =>
      completeSetup(requireInstance(instance), sessionToken),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey })
    },
  })
}
