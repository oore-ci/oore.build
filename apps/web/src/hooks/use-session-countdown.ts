import { useEffect, useState } from 'react'
import { useSetupStore } from '@/stores/setup-store'

interface SessionCountdown {
  remainingSeconds: number | null
  isExpired: boolean
  isWarning: boolean
  formatted: string | null
}

export function useSessionCountdown(): SessionCountdown {
  const sessionExpiresAt = useSetupStore((s) => s.sessionExpiresAt)
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null)

  useEffect(() => {
    if (sessionExpiresAt == null) {
      setRemainingSeconds(null)
      return
    }

    function tick() {
      const now = Math.floor(Date.now() / 1000)
      setRemainingSeconds(Math.max(0, sessionExpiresAt! - now))
    }

    tick()
    const id = setInterval(tick, 1000)
    return () => clearInterval(id)
  }, [sessionExpiresAt])

  if (remainingSeconds == null) {
    return {
      remainingSeconds: null,
      isExpired: false,
      isWarning: false,
      formatted: null,
    }
  }

  const isExpired = remainingSeconds <= 0
  const isWarning = remainingSeconds > 0 && remainingSeconds < 5 * 60
  const minutes = Math.floor(remainingSeconds / 60)
  const seconds = remainingSeconds % 60
  const formatted = `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`

  return { remainingSeconds, isExpired, isWarning, formatted }
}
