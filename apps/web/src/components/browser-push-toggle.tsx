import { useEffect, useState } from 'react'

import { Button } from '@/components/ui/button'
import { useWebPushConfig, useWebPushSubscription } from '@/hooks/use-testers'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { toast } from '@/lib/toast'
import {
  currentPushSubscription,
  subscribeThisBrowser,
  subscriptionKeys,
  webPushSupported,
} from '@/lib/web-push'

/**
 * New builds as a notification on this device, even with Oore closed. On
 * iPhone it works once Oore is added to the Home Screen.
 */
export function useBrowserPush() {
  const supported = webPushSupported()
  const [endpoint, setEndpoint] = useState<string | null>(null)
  const [working, setWorking] = useState(false)
  const configQuery = useWebPushConfig(endpoint)
  const { subscribe, unsubscribe, test } = useWebPushSubscription()

  useEffect(() => {
    void currentPushSubscription().then((subscription) =>
      setEndpoint(subscription?.endpoint ?? null),
    )
  }, [])

  const subscribed = !!endpoint && configQuery.data?.subscribed === true

  async function turnOn() {
    const publicKey = configQuery.data?.public_key
    if (!publicKey) return
    setWorking(true)
    try {
      const subscription = await subscribeThisBrowser(publicKey)
      const keys = subscriptionKeys(subscription)
      if (!keys) throw new Error('This browser did not return push keys.')
      await subscribe.mutateAsync(keys)
      setEndpoint(keys.endpoint)
      toast.success('This device will hear about new builds')
    } catch (error) {
      toast.error(getApiErrorMessage(error, {}))
    } finally {
      setWorking(false)
    }
  }

  async function turnOff() {
    setWorking(true)
    try {
      const subscription = await currentPushSubscription()
      if (subscription) {
        await unsubscribe.mutateAsync(subscription.endpoint)
        await subscription.unsubscribe()
      }
      setEndpoint(null)
    } catch (error) {
      toast.error(getApiErrorMessage(error, {}))
    } finally {
      setWorking(false)
    }
  }

  return {
    supported,
    subscribed,
    working: working || configQuery.isLoading,
    toggle: (on: boolean) => void (on ? turnOn() : turnOff()),
    sendTest: () =>
      test.mutate(undefined, {
        onSuccess: () => toast.success('Test notification sent'),
        onError: (error) => toast.error(getApiErrorMessage(error, {})),
      }),
    testing: test.isPending,
  }
}

export function SendTestPushButton({
  onClick,
  pending,
}: {
  onClick: () => void
  pending: boolean
}) {
  return (
    <Button
      size="sm"
      variant="outline"
      className="self-start"
      disabled={pending}
      onClick={onClick}
    >
      Send a test
    </Button>
  )
}
