import { useEffect, useRef, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'

import type { Instance, SetupStatus } from '@/lib/types'
import { localLogin, trustedProxyLogin } from '@/lib/api'
import { isLoopbackHostname, resolveUrlHostname } from '@/lib/connectivity'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useAuthStore } from '@/stores/auth-store'

/**
 * Sanctioned reactive effect for the index page auth guard.
 *
 * This MUST re-run when `status` changes (it arrives asynchronously from
 * useSetupStatus()). A mount-only effect would never execute the guard logic
 * because `status` is undefined on the initial render.
 *
 * Handles three cases:
 * 1. Redirect to /setup when in setup mode (non-local)
 * 2. Auto local-login for loopback instances in local runtime mode
 * 3. Auto trusted-proxy login for configured proxy-authenticated instances
 * 4. Redirect to /login when token is expired on other configured instances
 */
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

      if (hasValidToken) return
      if (autoLoginInstanceRef.current === instance.id) return

      autoLoginInstanceRef.current = instance.id
      setIsAutoSigningIn(true)
      clearAuth()
      void localLogin(baseUrl, {})
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
              avatar_url: response.user.avatar_url,
            },
            'local',
          )
        })
        .catch(() => {
          autoLoginInstanceRef.current = null
          clearAuth()
          void navigate({ to: '/login' })
        })
        .finally(() => {
          setIsAutoSigningIn(false)
        })
      return
    }

    if (status.remote_auth_mode === 'trusted_proxy') {
      if (hasValidToken) return
      if (!status.is_configured) return
      if (autoLoginInstanceRef.current === instance.id) return

      autoLoginInstanceRef.current = instance.id
      setIsAutoSigningIn(true)
      clearAuth()
      void trustedProxyLogin(baseUrl)
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
              avatar_url: response.user.avatar_url,
            },
            'trusted_proxy',
          )
        })
        .catch(() => {
          autoLoginInstanceRef.current = null
          clearAuth()
          void navigate({ to: '/login' })
        })
        .finally(() => {
          setIsAutoSigningIn(false)
        })
      return
    }

    if (status.is_configured && !hasValidToken) {
      clearAuth()
      void navigate({ to: '/login' })
    }
  }, [
    status,
    instance,
    authToken,
    authExpiresAt,
    navigate,
    clearAuth,
    setAuth,
    setIsAutoSigningIn,
  ])

  return isAutoSigningIn
}
