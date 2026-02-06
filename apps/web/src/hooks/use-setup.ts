import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { Instance, OidcConfigureRequest } from '@/lib/types'
import {
  completeSetup,
  configureOidc,
  getSetupStatus,
  setupOidcStart,
  setupOidcVerify,
  verifyBootstrapToken,
} from '@/lib/api'
import { useActiveInstance } from '@/stores/instance-store'
import { useSetupStore } from '@/stores/setup-store'

function requireInstance(instance: Instance | null): string {
  if (!instance)
    throw new Error('No active instance. Select or add an instance first.')
  return instance.url
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
