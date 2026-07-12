import {
  Outlet,
  createFileRoute,
  isRedirect,
  redirect,
  useNavigate,
} from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Tick02Icon } from '@hugeicons/core-free-icons'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { useSetupStatus } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { useSessionCountdown } from '@/hooks/use-session-countdown'
import { useExpiredSetupSessionRedirect } from '@/hooks/use-setup-route-transitions'
import { getSetupStatus } from '@/lib/api'
import {
  getConnectivityIssue,
  isHostedUiOrigin,
  isMixedContentBlocked,
} from '@/lib/connectivity'
import {
  getActiveInstanceOrRedirect,
  syncSetupStoreContext,
} from '@/lib/instance-context'
import {
  normalizeTrustedProxySetupPreset,
  saveTrustedProxySetupPrefill,
} from '@/lib/setup-prefill'
import {
  resolveInstanceApiBaseUrl,
  resolveRequiredInstanceApiBaseUrl,
} from '@/lib/instance-url'
import { useInstanceStore } from '@/stores/instance-store'
import { PageMeta } from '@/lib/seo'

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
  errorComponent: SetupError,
})

function StepIndicator({
  currentStep,
  steps,
}: {
  currentStep: number
  steps: Array<string>
}) {
  return (
    <div className="flex items-center justify-center gap-1">
      {steps.map((label, index) => {
        const isActive = index === currentStep
        const isCompleted = index < currentStep

        return (
          <div key={label} className="flex items-center gap-1">
            {index > 0 ? (
              <div
                className={`h-px w-8 ${isCompleted ? 'bg-primary' : 'bg-border'}`}
              />
            ) : null}
            <Badge
              variant={
                isActive ? 'default' : isCompleted ? 'secondary' : 'outline'
              }
              className="text-xs"
            >
              {isCompleted ? (
                <>
                  <HugeiconsIcon
                    icon={Tick02Icon}
                    size={12}
                    className="mr-0.5"
                  />
                  {label}
                </>
              ) : (
                label
              )}
            </Badge>
          </div>
        )
      })}
    </div>
  )
}

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

        <StepIndicator currentStep={currentStep} steps={steps} />

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

function SetupError({ error }: { error: Error }) {
  const navigate = useNavigate()
  const activeInstanceId = useInstanceStore((s) => s.activeInstanceId)
  const instances = useInstanceStore((s) => s.instances)
  const instance = activeInstanceId ? instances[activeInstanceId] : null
  const backendUrl = resolveInstanceApiBaseUrl(instance) ?? ''
  const frontendOrigin = window.location.origin
  const issue =
    backendUrl.length > 0
      ? getConnectivityIssue(backendUrl, error, frontendOrigin)
      : null
  const hostedUi = isHostedUiOrigin(frontendOrigin)

  if (!issue) {
    return (
      <div className="flex-1 flex items-center justify-center p-6">
        <div className="w-full max-w-xl space-y-4">
          <Alert variant="destructive">
            <AlertTitle>Something went wrong</AlertTitle>
            <AlertDescription>{error.message}</AlertDescription>
          </Alert>
          <Button onClick={() => void navigate({ to: '/setup' })}>
            Retry setup
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className="flex-1 flex items-center justify-center p-6">
      <div className="w-full max-w-xl space-y-4">
        <Alert variant="destructive">
          <AlertTitle>{issue.title}</AlertTitle>
          <AlertDescription>{issue.description}</AlertDescription>
        </Alert>

        <Card>
          <CardContent className="space-y-4">
            <div className="space-y-1">
              <p className="text-sm font-medium">Use CLI setup</p>
              <p className="text-sm text-muted-foreground">
                Complete first-run setup directly on the backend host:
              </p>
              <code className="block bg-muted px-2 py-1 text-xs">
                oore setup
              </code>
            </div>

            <div className="space-y-1">
              <p className="text-sm font-medium">Expose backend over HTTPS</p>
              <p className="text-sm text-muted-foreground">
                Use a tunnel and reconnect with the assigned HTTPS URL:
              </p>
              <code className="block bg-muted px-2 py-1 text-xs">
                cloudflared tunnel --url {backendUrl}
              </code>
            </div>

            {hostedUi ? (
              <div className="space-y-1">
                <p className="text-sm font-medium">
                  Use local/self-hosted web UI
                </p>
                <p className="text-sm text-muted-foreground">
                  If backend stays local-only, run the bundled local web
                  launcher:
                </p>
                <code className="block bg-muted px-2 py-1 text-xs">
                  oore-web --backend-url {backendUrl}
                </code>
              </div>
            ) : null}
          </CardContent>
        </Card>

        <div className="flex gap-2">
          <Button onClick={() => void navigate({ to: '/setup' })}>
            Retry setup
          </Button>
          <Button
            variant="outline"
            onClick={() => window.open('https://docs.oore.build', '_blank')}
          >
            Open docs
          </Button>
        </div>
      </div>
    </div>
  )
}
