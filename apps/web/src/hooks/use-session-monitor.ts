import { useEffect, useRef } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { toast } from 'sonner'
import { useAuthStore } from '@/stores/auth-store'

const WARN_BEFORE_S = 5 * 60 // 5 minutes
const CHECK_INTERVAL_MS = 30_000 // 30 seconds

export function useSessionMonitor() {
  const navigate = useNavigate()
  const warnedRef = useRef(false)

  useEffect(() => {
    const id = setInterval(() => {
      const { token, expiresAt, clearAuth } = useAuthStore.getState()
      if (!token || expiresAt == null) {
        warnedRef.current = false
        return
      }

      const now = Math.floor(Date.now() / 1000)
      const remaining = expiresAt - now

      if (remaining <= 0) {
        clearAuth()
        toast.error('Session expired. Please sign in again.')
        void navigate({ to: '/login' })
        warnedRef.current = false
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
  }, [navigate])
}
