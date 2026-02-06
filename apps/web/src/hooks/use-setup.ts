import {
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import {
  completeSetup,
  configureOidc,
  getSetupStatus,
  setupOidcStart,
  setupOidcVerify,
  verifyBootstrapToken,
} from '@/lib/api'
import type { OidcConfigureRequest } from '@/lib/types'
import { useSetupStore } from '@/stores/setup-store'

const SETUP_STATUS_KEY = ['setup-status'] as const

export function useSetupStatus() {
  return useQuery({
    queryKey: SETUP_STATUS_KEY,
    queryFn: getSetupStatus,
    refetchInterval: 3000,
  })
}

export function useVerifyBootstrapToken() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (token: string) => verifyBootstrapToken(token),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SETUP_STATUS_KEY })
    },
  })
}

export function useConfigureOidc() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      sessionToken,
      data,
    }: {
      sessionToken: string
      data: OidcConfigureRequest
    }) => configureOidc(sessionToken, data),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey: SETUP_STATUS_KEY })
    },
  })
}

export function useSetupOidcStart() {
  return useMutation({
    mutationFn: ({
      sessionToken,
      redirectUri,
    }: {
      sessionToken: string
      redirectUri: string
    }) => setupOidcStart(sessionToken, redirectUri),
  })
}

export function useSetupOidcVerify() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      sessionToken,
      code,
      state,
    }: {
      sessionToken: string
      code: string
      state: string
    }) => setupOidcVerify(sessionToken, code, state),
    onSuccess: (data) => {
      if (data.session_expires_at) {
        useSetupStore.getState().setSessionExpiresAt(data.session_expires_at)
      }
      void queryClient.invalidateQueries({ queryKey: SETUP_STATUS_KEY })
    },
  })
}

export function useCompleteSetup() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (sessionToken: string) => completeSetup(sessionToken),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SETUP_STATUS_KEY })
    },
  })
}
