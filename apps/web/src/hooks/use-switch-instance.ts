import { useRouter } from '@tanstack/react-router'

import { hasValidSessionForInstance } from '@/lib/instance-context'
import { useInstanceStore } from '@/stores/instance-store'

/**
 * Switches the active instance from inside the app. Page data belongs to the
 * old instance, so this leaves the page first: an unsaved-changes guard can
 * stop the switch, and the new instance opens on Home, or on sign-in when it
 * has no session.
 */
export function useSwitchInstance() {
  const router = useRouter()

  return async function switchInstance(instanceId: string) {
    const store = useInstanceStore.getState()
    if (instanceId === store.activeInstanceId) return
    const { pathname } = router.state.location
    // Sign-in and setup pages already follow the active instance.
    if (pathname.startsWith('/login') || pathname.startsWith('/setup')) {
      store.setActiveInstance(instanceId)
      return
    }

    if (pathname !== '/') {
      await router.navigate({ to: '/' })
      // A guard kept the person on a page with unsaved changes.
      if (router.state.location.pathname !== '/') return
    }

    store.setActiveInstance(instanceId)
    if (!hasValidSessionForInstance(instanceId)) {
      await router.navigate({ to: '/login', replace: true })
    }
  }
}
