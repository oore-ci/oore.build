import {
  Outlet,
  createFileRoute,
  redirect,
  useLocation,
} from '@tanstack/react-router'
import { setupStatusQueryOptions, useSetupStatus } from '@/hooks/use-setup'
import { useSessionCountdown } from '@/hooks/use-session-countdown'
import { useExpiredSetupSessionRedirect } from '@/hooks/use-setup-route-transitions'
import { isLoopbackUrl, isMixedContentBlocked } from '@/lib/connectivity'
import {
  getActiveInstanceOrRedirect,
  syncSetupStoreContext,
} from '@/lib/instance-context'
import {
  resolveInstanceApiBaseUrl,
  resolveRequiredInstanceApiBaseUrl,
} from '@/lib/instance-url'
import { useInstanceStore } from '@/stores/instance-store'
import { useSetupStore } from '@/stores/setup-store'
import { PageMeta } from '@/lib/seo'
import { queryClient } from '@/lib/query-client'
import {
  SetupRouteError,
  SetupStepIndicator,
} from '@/components/setup-route-components'
import { FocusedFlowHeader } from '@/components/focused-flow-header'
import { FocusedFlowLayout } from '@/components/focused-flow-layout'

function normalizeHandoffBackend(value: string): string | null {
  try {
    const url = new URL(value)
    if (
      !['http:', 'https:'].includes(url.protocol) ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    ) {
      return null
    }
    return url.toString().replace(/\/+$/, '')
  } catch {
    return null
  }
}

function maybeAutoAddBackendInstance(): string | null {
  const params = new URLSearchParams(window.location.search)
  const backendUrl = params.get('backend')
  if (!backendUrl) return null

  const store = useInstanceStore.getState()
  const normalizedBackend = normalizeHandoffBackend(backendUrl)
  if (!normalizedBackend) return null
  const parsedBackendUrl = new URL(normalizedBackend)

  // Keep remote handoffs fail-closed when another instance is already saved.
  // A loopback setup page can safely activate its loopback control plane.
  if (Object.keys(store.instances).length === 0) {
    const label = isLoopbackUrl(backendUrl)
      ? 'Local'
      : parsedBackendUrl.hostname
    const id = store.addInstance(label, normalizedBackend)
    store.setActiveInstance(id)
  } else {
    const matchingInstance = Object.values(store.instances).find(
      (instance) =>
        normalizeHandoffBackend(resolveInstanceApiBaseUrl(instance) ?? '') ===
        normalizedBackend,
    )
    if (matchingInstance) {
      store.setActiveInstance(matchingInstance.id)
    } else if (
      isLoopbackUrl(normalizedBackend) &&
      isLoopbackUrl(window.location.origin)
    ) {
      const id = store.addInstance('Local', normalizedBackend)
      store.setActiveInstance(id)
    }
  }

  const url = new URL(window.location.href)
  url.searchParams.delete('backend')
  window.history.replaceState(
    window.history.state,
    '',
    url.pathname + url.search + url.hash,
  )
  return normalizedBackend
}

function takeBootstrapTokenFromFragment(): string | null | undefined {
  const fragment = new URLSearchParams(window.location.hash.slice(1))
  if (!fragment.has('bootstrap_token')) return undefined

  const token = fragment.get('bootstrap_token')
  fragment.delete('bootstrap_token')

  const url = new URL(window.location.href)
  const remainingFragment = fragment.toString()
  url.hash = remainingFragment ? `#${remainingFragment}` : ''
  window.history.replaceState(
    window.history.state,
    '',
    url.pathname + url.search + url.hash,
  )

  return token?.trim() || null
}

export const Route = createFileRoute('/setup')({
  beforeLoad: async () => {
    // Handle ?backend= query param before instance guards
    const requestedBackend = maybeAutoAddBackendInstance()
    const bootstrapToken = takeBootstrapTokenFromFragment()

    const instance = getActiveInstanceOrRedirect()
    const baseUrl = resolveRequiredInstanceApiBaseUrl(instance)
    syncSetupStoreContext(instance.id)
    if (bootstrapToken !== undefined) {
      const activeBackend = normalizeHandoffBackend(baseUrl)
      if (
        bootstrapToken &&
        (!requestedBackend || requestedBackend !== activeBackend)
      ) {
        throw new Error(
          'This setup link targets a different Oore instance. Select that instance, then generate a new setup link.',
        )
      }
      useSetupStore.getState().setBootstrapTokenPrefill(bootstrapToken)
    }
    if (isMixedContentBlocked(window.location.origin, baseUrl)) {
      throw new Error('mixed_content_blocked')
    }

    const status = await queryClient.ensureQueryData(
      setupStatusQueryOptions(instance),
    )
    if (status.is_configured) {
      useSetupStore.getState().setBootstrapTokenPrefill(null)
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
  const currentStepByPath = new Map([
    ['/setup', 0],
    ['/setup/', 0],
    ['/setup/mode', 1],
    ['/setup/oidc', 2],
    ['/setup/trusted-proxy', 2],
    ['/setup/owner', steps.length - 2],
    ['/setup/complete', steps.length - 1],
  ])
  const currentStep = status?.is_configured
    ? steps.length
    : (currentStepByPath.get(pathname) ?? 0)

  useExpiredSetupSessionRedirect(isExpired)

  return (
    <FocusedFlowLayout>
      <PageMeta title="Setup" />
      <div className="w-full max-w-lg space-y-5 sm:space-y-6">
        <FocusedFlowHeader
          title="Set up Oore"
          description="Configure your self-hosted CI instance."
        />

        <div className="space-y-2">
          <SetupStepIndicator currentStep={currentStep} steps={steps} />
          {formatted && !isExpired ? (
            <p
              className={`text-center text-xs tabular-nums ${isWarning ? 'font-semibold text-destructive' : 'text-muted-foreground'}`}
            >
              Session expires in {formatted}
            </p>
          ) : null}
        </div>

        <div className="setup-step-content border-t pt-5 sm:pt-6">
          <Outlet />
        </div>
      </div>
    </FocusedFlowLayout>
  )
}
