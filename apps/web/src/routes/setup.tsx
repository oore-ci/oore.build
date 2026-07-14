import {
  Outlet,
  createFileRoute,
  isRedirect,
  redirect,
} from '@tanstack/react-router'
import { Card, CardContent } from '@/components/ui/card'
import { useSetupStatus } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { useSessionCountdown } from '@/hooks/use-session-countdown'
import { useExpiredSetupSessionRedirect } from '@/hooks/use-setup-route-transitions'
import { getSetupStatus } from '@/lib/api'
import { isMixedContentBlocked } from '@/lib/connectivity'
import {
  getActiveInstanceOrRedirect,
  syncSetupStoreContext,
} from '@/lib/instance-context'
import {
  normalizeTrustedProxySetupPreset,
  saveTrustedProxySetupPrefill,
} from '@/lib/setup-prefill'
import { resolveRequiredInstanceApiBaseUrl } from '@/lib/instance-url'
import { useInstanceStore } from '@/stores/instance-store'
import { PageMeta } from '@/lib/seo'
import {
  SetupRouteError,
  SetupStepIndicator,
} from '@/components/setup-route-components'

function isLoopbackHost(hostname: string): boolean {
  return hostname === '127.0.0.1' || hostname === 'localhost'
}

function maybeAutoAddBackendInstance() {
  const params = new URLSearchParams(window.location.search)
  const backendUrl = params.get('backend')
  const ownerEmail = params.get('setup_owner_email')
  const proxyPreset = normalizeTrustedProxySetupPreset(
    params.get('proxy_preset'),
  )
  const userEmailHeader = params.get('user_email_header')
  const hasSetupPrefill = Boolean(ownerEmail || proxyPreset || userEmailHeader)
  if (!backendUrl && !hasSetupPrefill) return

  const store = useInstanceStore.getState()
  let instanceId = store.activeInstanceId

  if (backendUrl) {
    try {
      new URL(backendUrl)
    } catch {
      return
    }

    // Only auto-add if instance store is empty (prevents phishing via crafted links)
    if (Object.keys(store.instances).length === 0) {
      const parsed = new URL(backendUrl)
      const label = isLoopbackHost(parsed.hostname) ? 'Local' : parsed.hostname
      const id = store.addInstance(label, backendUrl.replace(/\/+$/, ''))
      store.setActiveInstance(id)
      instanceId = id
    }
  }

  if (instanceId && hasSetupPrefill) {
    saveTrustedProxySetupPrefill(instanceId, {
      ownerEmail: ownerEmail ?? undefined,
      proxyPreset,
      userEmailHeader: userEmailHeader ?? undefined,
    })
  }

  const url = new URL(window.location.href)
  url.searchParams.delete('backend')
  url.searchParams.delete('setup_owner_email')
  url.searchParams.delete('proxy_preset')
  url.searchParams.delete('user_email_header')
  window.history.replaceState({}, '', url.pathname + url.search)
}

export const Route = createFileRoute('/setup')({
  beforeLoad: async () => {
    // Handle ?backend= query param before instance guards
    maybeAutoAddBackendInstance()

    const instance = getActiveInstanceOrRedirect()
    const baseUrl = resolveRequiredInstanceApiBaseUrl(instance)
    syncSetupStoreContext(instance.id)
    if (isMixedContentBlocked(window.location.origin, baseUrl)) {
      throw new Error('mixed_content_blocked')
    }

    try {
      const status = await getSetupStatus(baseUrl)
      if (status.is_configured) {
        throw redirect({ to: '/' })
      }
    } catch (e) {
      if (isRedirect(e)) throw e
      throw e
    }
  },
  component: SetupLayout,
  errorComponent: SetupRouteError,
})

function SetupLayout() {
  const currentStep = useSetupStore((s) => s.currentStep)
  const { data: status } = useSetupStatus()
  const { formatted, isWarning, isExpired } = useSessionCountdown()
  const steps =
    status?.runtime_mode === 'local'
      ? ['Token', 'Mode', 'Owner', 'Complete']
      : status?.remote_auth_mode === 'trusted_proxy'
        ? ['Token', 'Mode', 'Proxy', 'Owner', 'Complete']
        : ['Token', 'Mode', 'OIDC', 'Owner', 'Complete']

  useExpiredSetupSessionRedirect(isExpired)

  return (
    <div className="flex-1 flex flex-col items-center justify-center p-6">
      <PageMeta title="Setup" />
      <div className="w-full max-w-lg space-y-8">
        <div className="text-center space-y-4">
          <div className="mx-auto flex size-14 items-center justify-center">
            <img src="/logo.svg" alt="Oore logo" className="size-full" />
          </div>
          <div className="space-y-1">
            <h1 className="text-3xl font-bold tracking-tight">
              Instance Setup
            </h1>
            <p className="text-muted-foreground text-sm">
              Configure your self-hosted CI instance
            </p>
          </div>
        </div>

        <SetupStepIndicator currentStep={currentStep} steps={steps} />

        {formatted && !isExpired ? (
          <div className="text-center">
            <p
              className={`text-xs font-mono ${isWarning ? 'text-destructive font-semibold' : 'text-muted-foreground'}`}
            >
              Session expires in {formatted}
            </p>
          </div>
        ) : null}

        <Card>
          <CardContent>
            <Outlet />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
