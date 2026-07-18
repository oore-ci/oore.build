import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { WifiDisconnected04Icon } from '@hugeicons/core-free-icons'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { Alert, AlertDescription } from '@/components/ui/alert'

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
    <Alert
      aria-live="assertive"
      className="sticky top-0 z-40 place-content-center rounded-none border-x-0 border-t-0 border-destructive/30 bg-destructive/10 py-2 text-destructive"
      variant="destructive"
    >
      <HugeiconsIcon icon={WifiDisconnected04Icon} aria-hidden />
      <AlertDescription className="text-destructive">
        You are offline
      </AlertDescription>
    </Alert>
  )
}
