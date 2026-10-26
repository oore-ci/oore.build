import { useEffect, useRef, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'

import type { Instance } from '@/lib/types'
import type { SetupStatus } from '@oore/client/models'
import { localLogin, trustedProxyLogin } from '@oore/client/operations'
import { isLoopbackHostname, resolveUrlHostname } from '@/lib/connectivity'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { createWebOoreClient } from '@/lib/api-client/client'
import { useAuthStore } from '@/stores/auth-store'

// Setup status arrives asynchronously, so authentication must react to it.
export function useIndexAuthGuard(
  status: SetupStatus | undefined,
  instance: Instance | null,
) {
  const [isAutoSigningIn, setIsAutoSigningIn] = useState(false)
  const navigate = useNavigate()
  const autoLoginInstanceRef = useRef<string | null>(null)
  const authToken = useAuthStore((s) => s.token)
  const authExpiresAt = useAuthStore((s) => s.expiresAt)
  const clearAuth = useAuthStore((s) => s.clearAuth)
  const setAuth = useAuthStore((s) => s.setAuth)

  useEffect(() => {
    if (!status || !instance) return
    const baseUrl = resolveInstanceApiBaseUrl(instance)
    if (!baseUrl) return
    const client = createWebOoreClient({ baseUrl })

    if (status.setup_mode && status.runtime_mode !== 'local') {
      void navigate({ to: '/setup' })
      return
    }

    const now = Math.floor(Date.now() / 1000)
    const hasValidToken =
      !!authToken && authExpiresAt != null && authExpiresAt > now

    if (status.runtime_mode === 'local') {
      const uiIsLoopback = isLoopbackHostname(window.location.hostname)
      const backendIsLoopback = isLoopbackHostname(resolveUrlHostname(baseUrl))

      if (!uiIsLoopback || !backendIsLoopback) {
        if (!hasValidToken) {
          clearAuth()
          void navigate({ to: '/login' })
        }
        return
      }
    } else if (status.remote_auth_mode !== 'trusted_proxy') {
      if (status.is_configured && !hasValidToken) {
        clearAuth()
        void navigate({ to: '/login' })
      }
      return
    } else if (!status.is_configured) {
      return
    }

    if (hasValidToken || autoLoginInstanceRef.current === instance.id) return

    autoLoginInstanceRef.current = instance.id
    setIsAutoSigningIn(true)
    clearAuth()
    const method = status.runtime_mode === 'local' ? 'local' : 'trusted_proxy'
    const login =
      method === 'local'
        ? localLogin({ body: {}, client })
        : trustedProxyLogin({ client })

    void login
      .then((response) => {
        if (!response.user.user_id || !response.user.role) {
          throw new Error('Incomplete user profile received from server')
        }
        setAuth(
          response.session_token,
          response.expires_at,
          {
            email: response.user.email,
            oidc_subject: response.user.oidc_subject,
            user_id: response.user.user_id,
            role: response.user.role,
            avatar_url: response.user.avatar_url ?? undefined,
          },
          method,
        )
      })
      .catch(() => {
        autoLoginInstanceRef.current = null
        clearAuth()
        void navigate({ to: '/login' })
      })
      .finally(() => setIsAutoSigningIn(false))
  }, [status, instance, authToken, authExpiresAt, navigate, clearAuth, setAuth])

  return isAutoSigningIn
}
