import { lazy, Suspense, useState } from 'react'

import { useMountEffect } from '@/hooks/use-mount-effect'

const Toaster = lazy(() =>
  import('@/components/ui/sonner').then((module) => ({
    default: module.Toaster,
  })),
)

export default function DeferredToaster() {
  const [ready, setReady] = useState(false)
  useMountEffect(() => {
    const timeout = window.setTimeout(() => setReady(true), 0)
    return () => window.clearTimeout(timeout)
  })
  return ready ? (
    <Suspense fallback={null}>
      <Toaster />
    </Suspense>
  ) : null
}
