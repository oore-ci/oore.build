import { useEffect, useRef } from 'react'

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
    if (
      typeof Notification !== 'undefined' &&
      Notification.permission === 'default'
    ) {
      void Notification.requestPermission()
    }
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
      typeof Notification !== 'undefined' &&
      Notification.permission === 'granted'
    ) {
      new Notification(`Build #${build_number} ${status}`, {
        body: `Branch: ${branch ?? 'n/a'}`,
      })
    }
  }, [build, isTerminal])
}
