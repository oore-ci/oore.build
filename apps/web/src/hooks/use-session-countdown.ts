import { useSetupStore } from '@/stores/setup-store'
import { useTime } from './use-time'

interface SessionCountdown {
  remainingSeconds: number | null
  isExpired: boolean
  isWarning: boolean
  formatted: string | null
}

export function useSessionCountdown(): SessionCountdown {
  const sessionExpiresAt = useSetupStore((s) => s.sessionExpiresAt)
  const time = useTime()
  const now = Math.floor(time / 1000)
  const remainingSeconds =
    sessionExpiresAt == null ? null : Math.max(0, sessionExpiresAt - now)

  if (remainingSeconds == null) {
    return {
      remainingSeconds: null,
      isExpired: false,
      isWarning: false,
      formatted: null,
    }
  }

  const formatted = formatRemaining(remainingSeconds)

  return {
    remainingSeconds,
    isExpired: remainingSeconds <= 0,
    isWarning: remainingSeconds > 0 && remainingSeconds < 5 * 60,
    formatted,
  }
}

const SECONDS_PER_DAY = 86_400

/** `m:ss` under an hour, `h:mm:ss` under a day, then whole days and hours. */
export function formatRemaining(totalSeconds: number): string {
  if (totalSeconds >= SECONDS_PER_DAY) {
    const days = Math.floor(totalSeconds / SECONDS_PER_DAY)
    const hours = Math.floor((totalSeconds % SECONDS_PER_DAY) / 3600)
    return hours > 0 ? `${days}d ${hours}h` : `${days}d`
  }
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = String(totalSeconds % 60).padStart(2, '0')
  if (hours === 0) return `${minutes}:${seconds}`
  return `${hours}:${String(minutes).padStart(2, '0')}:${seconds}`
}
