import { useMutation } from '@tanstack/react-query'

import type { GitHubAppStartRequest } from '@/lib/types'
import { githubAppStart, setupOidcStart } from '@/lib/api'
import { resolveRequiredInstanceApiBaseUrl } from '@/lib/instance-url'
import { useActiveInstance } from '@/stores/instance-store'
import { useApiContext } from '@/hooks/use-api-context'

export function usePreviewGitHubAppSetup() {
  const { baseUrl, token } = useApiContext()

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
    }) =>
      setupOidcStart(
        resolveRequiredInstanceApiBaseUrl(instance),
        sessionToken,
        redirectUri,
      ),
  })
}
