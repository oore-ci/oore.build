import { useEffect, useRef, useState } from 'react'

interface BuildInfo {
  build_number: number
  status: string
  branch?: string | null
}

export function useBuildNotification(
  build: BuildInfo | undefined,
  isTerminal: boolean,
) {
  const prevStatusRef = useRef<string | undefined>(undefined)

  useEffect(() => {
    return () => {
      document.title = 'Oore CI'
    }
  }, [])

  useEffect(() => {
    if (!build) return

    const { build_number, status, branch } = build

    switch (status) {
      case 'running':
      case 'queued':
        document.title = `\u23F3 Build #${build_number} | Oore CI`
        break
      case 'succeeded':
        document.title = `\u2713 Build #${build_number} | Oore CI`
        break
      case 'failed':
      case 'timed_out':
      case 'expired':
        document.title = `\u2717 Build #${build_number} | Oore CI`
        break
      case 'canceled':
        document.title = `\u2298 Build #${build_number} | Oore CI`
        break
      default:
        document.title = `Build #${build_number} | Oore CI`
    }

    const prevStatus = prevStatusRef.current
    prevStatusRef.current = status

    if (
      prevStatus !== undefined &&
      prevStatus !== status &&
      isTerminal &&
      document.hidden &&
      globalThis.Notification !== undefined &&
      globalThis.Notification.permission === 'granted'
    ) {
      new Notification(`Build #${build_number} ${status}`, {
        body: `Branch: ${branch ?? 'n/a'}`,
      })
    }
  }, [build, isTerminal])
}

/**
 * Browsers only show the permission prompt after a user gesture, so ask from
 * an explicit control rather than on page load.
 */
export function useNotificationPermission() {
  const supported = globalThis.Notification !== undefined
  const [permission, setPermission] = useState<
    NotificationPermission | 'unsupported'
  >(supported ? Notification.permission : 'unsupported')

  function request() {
    if (!supported) return
    void Notification.requestPermission().then(setPermission)
  }

  return { permission, request }
}
