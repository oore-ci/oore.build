import { useEffect, useRef } from 'react'
import { useNavigate } from '@tanstack/react-router'

import type { Instance, SetupStatus } from '@/lib/types'
import { localLogin } from '@/lib/api'
import { useAuthStore } from '@/stores/auth-store'

function isLoopbackHostname(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '::1' ||
    hostname === '[::1]'
  )
}

function resolveBackendHostname(rawUrl: string): string {
  const trimmed = rawUrl.trim()
  if (!trimmed) return window.location.hostname
  try {
    return new URL(trimmed).hostname
  } catch {
    return ''
  }
}

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
 * 3. Redirect to /login when token is expired on a configured instance
 */
export function useIndexAuthGuard(
  status: SetupStatus | undefined,
  instance: Instance | null,
  setIsAutoLocalSigningIn: (v: boolean) => void,
) {
  const navigate = useNavigate()
  const autoLocalLoginInstanceRef = useRef<string | null>(null)
  const authToken = useAuthStore((s) => s.token)
  const authExpiresAt = useAuthStore((s) => s.expiresAt)
  const clearAuth = useAuthStore((s) => s.clearAuth)
  const setAuth = useAuthStore((s) => s.setAuth)

  useEffect(() => {
    if (!status || !instance) return

    if (status.setup_mode && status.runtime_mode !== 'local') {
      void navigate({ to: '/setup' })
      return
    }

    const now = Math.floor(Date.now() / 1000)
    const hasValidToken =
      !!authToken && authExpiresAt != null && authExpiresAt > now

    if (status.runtime_mode === 'local') {
      const uiIsLoopback = isLoopbackHostname(window.location.hostname)
      const backendIsLoopback = isLoopbackHostname(
        resolveBackendHostname(instance.url),
      )

      if (!uiIsLoopback || !backendIsLoopback) {
        if (!hasValidToken) {
          clearAuth()
          void navigate({ to: '/login' })
        }
        return
      }

      if (hasValidToken) return
      if (autoLocalLoginInstanceRef.current === instance.id) return

      autoLocalLoginInstanceRef.current = instance.id
      setIsAutoLocalSigningIn(true)
      clearAuth()
      void localLogin(instance.url, {})
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
          autoLocalLoginInstanceRef.current = null
          clearAuth()
          void navigate({ to: '/login' })
        })
        .finally(() => {
          setIsAutoLocalSigningIn(false)
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
    setIsAutoLocalSigningIn,
  ])

  return autoLocalLoginInstanceRef
}
