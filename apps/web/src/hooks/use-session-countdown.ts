import { createMemo, createSignal, onCleanup } from 'solid-js'
import { useSetupStore } from '@/stores/setup-store'

export function useSessionCountdown() {
  const expiresAt = useSetupStore((state) => state.sessionExpiresAt)
  const [now, setNow] = createSignal(Date.now())

  const interval = setInterval(() => {
    setNow(Date.now())
  }, 1_000)
  onCleanup(() => clearInterval(interval))

  const remainingSeconds = createMemo(() => {
    const expiry = expiresAt()
    if (expiry == null) return null
    return Math.floor(expiry - now() / 1000)
  })

  const isExpired = createMemo(() => {
    const remaining = remainingSeconds()
    return remaining != null && remaining <= 0
  })

  const isWarning = createMemo(() => {
    const remaining = remainingSeconds()
    return remaining != null && remaining <= 5 * 60
  })

  const formatted = createMemo(() => {
    const remaining = remainingSeconds()
    if (remaining == null || remaining <= 0) return null
    const minutes = Math.floor(remaining / 60)
    const seconds = remaining % 60
    return `${minutes.toString().padStart(2, '0')}:${seconds
      .toString()
      .padStart(2, '0')}`
  })

  return {
    formatted,
    isExpired,
    isWarning,
  }
}
