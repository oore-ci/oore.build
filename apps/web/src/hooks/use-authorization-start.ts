import { useMutation } from '@tanstack/react-query'

import type { GitHubAppStartRequest, Instance } from '@/lib/types'
import { githubAppStart, setupOidcStart } from '@/lib/api'
import {
  resolveInstanceApiBaseUrl,
  resolveRequiredInstanceApiBaseUrl,
} from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'

function useAuthToken(): string | null {
  const token = useAuthStore((state) => state.token)
  const expiresAt = useAuthStore((state) => state.expiresAt)
  if (!token || expiresAt == null) return null
  if (expiresAt <= Math.floor(Date.now() / 1000)) return null
  return token
}

function requireInstance(instance: Instance | null): string {
  return resolveRequiredInstanceApiBaseUrl(instance)
}

export function usePreviewGitHubAppSetup() {
  const instance = useActiveInstance()
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const token = useAuthToken()

  return useMutation({
    mutationFn: (data: GitHubAppStartRequest) => {
      if (!baseUrl || !token) {
        return Promise.reject(new Error('Not authenticated'))
      }
      return githubAppStart(baseUrl, token, data)
    },
  })
}

export function useSetupOidcVerificationStart() {
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
