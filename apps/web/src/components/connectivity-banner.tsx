import { useEffect, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { WifiDisconnected04Icon } from '@hugeicons/core-free-icons'

import { queryClient } from '@/lib/query-client'

export default function ConnectivityBanner() {
  const [offline, setOffline] = useState(
    typeof navigator !== 'undefined' ? !navigator.onLine : false,
  )
  const [daemonUnreachable, setDaemonUnreachable] = useState(false)

  useEffect(() => {
    const goOnline = () => {
      setOffline(false)
      setDaemonUnreachable(false)
    }
    const goOffline = () => setOffline(true)
    window.addEventListener('online', goOnline)
    window.addEventListener('offline', goOffline)
    return () => {
      window.removeEventListener('online', goOnline)
      window.removeEventListener('offline', goOffline)
    }
  }, [])

  useEffect(() => {
    let consecutiveFailures = 0
    const unsubscribe = queryClient.getQueryCache().subscribe((event) => {
      if (event.type === 'updated' && event.action.type === 'error') {
        consecutiveFailures++
        if (consecutiveFailures >= 3) setDaemonUnreachable(true)
      }
      if (event.type === 'updated' && event.action.type === 'success') {
        consecutiveFailures = 0
        setDaemonUnreachable(false)
      }
    })
    return unsubscribe
  }, [])

  if (!offline && !daemonUnreachable) return null

  return (
    <div className="sticky top-0 z-40 flex items-center justify-center gap-2 border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive">
      <HugeiconsIcon icon={WifiDisconnected04Icon} size={16} />
      {offline
        ? 'You are offline'
        : 'Backend is unreachable — is oored running?'}
    </div>
  )
}
