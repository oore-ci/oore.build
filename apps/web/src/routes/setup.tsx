import {
  Outlet,
  createFileRoute,
  redirect,
  useLocation,
} from '@tanstack/react-router'
import { Card, CardContent } from '@/components/ui/card'
import { setupStatusQueryOptions, useSetupStatus } from '@/hooks/use-setup'
import { useSessionCountdown } from '@/hooks/use-session-countdown'
import { useExpiredSetupSessionRedirect } from '@/hooks/use-setup-route-transitions'
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
import { queryClient } from '@/lib/query-client'
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

    const status = await queryClient.ensureQueryData(
      setupStatusQueryOptions(instance),
    )
    if (status.is_configured) {
      throw redirect({ to: '/' })
    }
  },
  component: SetupLayout,
  errorComponent: SetupRouteError,
})

function SetupLayout() {
  const pathname = useLocation({ select: (location) => location.pathname })
  const { data: status } = useSetupStatus()
  const { formatted, isWarning, isExpired } = useSessionCountdown()
  const steps =
    status?.runtime_mode === 'local'
      ? ['Token', 'Mode', 'Owner', 'Complete']
      : status?.remote_auth_mode === 'trusted_proxy'
        ? ['Token', 'Mode', 'Proxy', 'Owner', 'Complete']
        : ['Token', 'Mode', 'OIDC', 'Owner', 'Complete']
  const currentStepByPath: Record<string, number> = {
    '/setup': 0,
    '/setup/': 0,
    '/setup/mode': 1,
    '/setup/oidc': 2,
    '/setup/trusted-proxy': 2,
    '/setup/owner': steps.length - 2,
    '/setup/complete': steps.length - 1,
  }
  const currentStep = status?.is_configured
    ? steps.length
    : (currentStepByPath[pathname] ?? 0)

  useExpiredSetupSessionRedirect(isExpired)

  return (
    <div className="focused-flow flex min-h-0 flex-1 flex-col items-center p-4 sm:p-6">
      <PageMeta title="Setup" />
      <div className="w-full max-w-lg space-y-6 sm:space-y-8">
        <div className="space-y-4 text-center">
          <div className="mx-auto flex size-14 items-center justify-center">
            <img src="/logo.svg" alt="Oore logo" className="size-full" />
          </div>
          <div className="space-y-1">
            <h1 className="text-3xl font-bold tracking-tight">
              Instance Setup
            </h1>
            <p className="text-sm text-muted-foreground">
              Configure your self-hosted CI instance
            </p>
          </div>
        </div>

        <SetupStepIndicator currentStep={currentStep} steps={steps} />

        {formatted && !isExpired ? (
          <div className="text-center">
            <p
              className={`font-mono text-xs ${isWarning ? 'text-destructive font-semibold' : 'text-muted-foreground'}`}
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
