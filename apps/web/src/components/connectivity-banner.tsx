import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { WifiDisconnected04Icon } from '@hugeicons/core-free-icons'
import { useMountEffect } from '@/hooks/use-mount-effect'

export default function ConnectivityBanner() {
  const [offline, setOffline] = useState(
    typeof navigator !== 'undefined' ? !navigator.onLine : false,
  )

  useMountEffect(() => {
    const goOnline = () => setOffline(false)
    const goOffline = () => setOffline(true)
    window.addEventListener('online', goOnline)
    window.addEventListener('offline', goOffline)
    return () => {
      window.removeEventListener('online', goOnline)
      window.removeEventListener('offline', goOffline)
    }
  })

  if (!offline) return null

  return (
    <div className="sticky top-0 z-40 flex items-center justify-center gap-2 border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive">
      <HugeiconsIcon icon={WifiDisconnected04Icon} size={16} />
      You are offline
    </div>
  )
}
