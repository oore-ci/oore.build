import { useRef } from 'react'
import { useRouter } from '@tanstack/react-router'
import { toast } from '@/lib/toast'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { loginSearchFor } from '@/lib/auth-redirect'
import { onSessionEnded, reportSessionEnded } from '@/lib/session-events'
import { useAuthStore } from '@/stores/auth-store'

const WARN_BEFORE_S = 5 * 60 // 5 minutes
const CHECK_INTERVAL_MS = 30_000 // 30 seconds

const SESSION_END_MESSAGES = {
  expired: 'Your session expired. Sign in again to continue.',
  rejected: 'Your session is no longer valid. Sign in again to continue.',
}

/**
 * Sends the person to sign-in, back to where they were, when their session
 * expires locally or the daemon rejects it.
 */
export function useSessionMonitor() {
  const router = useRouter()
  const warnedRef = useRef(false)

  useMountEffect(() =>
    onSessionEnded(({ reason, token }) => {
      const { token: activeToken, clearAuth } = useAuthStore.getState()
      // Late responses for a session that already ended, or for an older one.
      if (!activeToken || activeToken !== token) return

      warnedRef.current = false
      clearAuth()
      toast.error(SESSION_END_MESSAGES[reason], {
        id: 'session-ended',
        duration: 10_000,
      })
      void router.navigate({
        to: '/login',
        search: loginSearchFor(router.state.location.href),
        replace: true,
      })
    }),
  )

  useMountEffect(() => {
    const id = setInterval(() => {
      const { token, expiresAt } = useAuthStore.getState()
      if (!token || expiresAt == null) {
        warnedRef.current = false
        return
      }

      const now = Math.floor(Date.now() / 1000)
      const remaining = expiresAt - now

      if (remaining <= 0) {
        reportSessionEnded({ reason: 'expired', token })
        return
      }

      if (remaining <= WARN_BEFORE_S && !warnedRef.current) {
        warnedRef.current = true
        const mins = Math.ceil(remaining / 60)
        toast.warning(
          `Session expires in ${mins} minute${mins === 1 ? '' : 's'}. Save your work.`,
        )
      }
    }, CHECK_INTERVAL_MS)

    return () => clearInterval(id)
  })
}
