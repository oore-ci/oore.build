import { useEffect, useEffectEvent, useRef } from 'react'

/**
 * Sanctioned reactive effect for proxy-authenticated login surfaces.
 *
 * When setup status proves that the active instance is configured for trusted
 * proxy auth, there is no separate OIDC/local choice to show. The app should
 * exchange the forwarded identity header for an Oore session once per instance.
 */
export function useTrustedProxyAutoLogin({
  enabled,
  instanceId,
  onLogin,
}: {
  enabled: boolean
  instanceId: string | null
  onLogin: () => void | Promise<void>
}) {
  const attemptedInstanceRef = useRef<string | null>(null)
  const login = useEffectEvent(onLogin)

  useEffect(() => {
    if (!enabled || !instanceId) return
    if (attemptedInstanceRef.current === instanceId) return

    attemptedInstanceRef.current = instanceId
    void login()
  }, [enabled, instanceId])
}
