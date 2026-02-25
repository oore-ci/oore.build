import { createEffect, For, Show } from 'solid-js'
import {
  Outlet,
  createFileRoute,
  isRedirect,
  redirect,
  useNavigate,
} from '@tanstack/solid-router'
import { Tick02Icon } from '@hugeicons/core-free-icons'
import { HugeIcon } from '@/components/huge-icon'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { useSetupStatus } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { useSessionCountdown } from '@/hooks/use-session-countdown'
import { getSetupStatus } from '@/lib/api'
import {
  getConnectivityIssue,
  isHostedUiOrigin,
  isMixedContentBlocked,
} from '@/lib/connectivity'
import { getActiveInstanceOrRedirect } from '@/lib/instance-context'
import { useInstanceStore } from '@/stores/instance-store'
import { PageMeta } from '@/lib/seo'

function isLoopbackHost(hostname: string): boolean {
  return hostname === '127.0.0.1' || hostname === 'localhost'
}

function maybeAutoAddBackendInstance() {
  const params = new URLSearchParams(window.location.search)
  const backendUrl = params.get('backend')
  if (!backendUrl) return

  try {
    new URL(backendUrl)
  } catch {
    return
  }

  const store = useInstanceStore.getState()
  if (Object.keys(store.instances).length > 0) return

  const parsed = new URL(backendUrl)
  const label = isLoopbackHost(parsed.hostname) ? 'Local' : parsed.hostname
  const id = store.addInstance(label, backendUrl.replace(/\/+$/, ''))
  store.setActiveInstance(id)

  const url = new URL(window.location.href)
  url.searchParams.delete('backend')
  window.history.replaceState({}, '', url.pathname + url.search)
}

export const Route = createFileRoute('/setup')({
  beforeLoad: async () => {
    maybeAutoAddBackendInstance()

    const instance = getActiveInstanceOrRedirect()
    if (isMixedContentBlocked(window.location.origin, instance.url)) {
      throw new Error('mixed_content_blocked')
    }

    try {
      const status = await getSetupStatus(instance.url)
      if (status.is_configured) {
        throw redirect({ to: '/' })
      }
    } catch (value) {
      if (isRedirect(value)) throw value
      throw value
    }
  },
  component: SetupLayout,
  errorComponent: SetupError,
})

function SetupLayout() {
  const currentStep = useSetupStore((state) => state.currentStep)
  const status = useSetupStatus()
  const { formatted, isWarning, isExpired } = useSessionCountdown()
  const navigate = useNavigate()

  const steps = () => {
    if (status.data?.runtime_mode === 'local') {
      return ['Token', 'Mode', 'Owner', 'Complete']
    }

    if (status.data?.remote_auth_mode === 'trusted_proxy') {
      return ['Token', 'Mode', 'Proxy', 'Owner', 'Complete']
    }

    return ['Token', 'Mode', 'OIDC', 'Owner', 'Complete']
  }

  createEffect(() => {
    if (isExpired()) {
      useSetupStore.getState().reset()
      void navigate({ to: '/setup' })
    }
  })

  return (
    <div class="flex min-h-screen flex-col items-center justify-center p-6">
      <PageMeta title="Setup" />
      <div class="w-full max-w-lg space-y-6">
        <div class="space-y-2 text-center">
          <div class="mx-auto flex size-14 items-center justify-center">
            <img src="/logo.svg" alt="Oore logo" class="size-full" />
          </div>
          <h1 class="text-3xl font-bold tracking-tight">Instance Setup</h1>
          <p class="text-sm text-muted-foreground">
            Configure your self-hosted CI instance.
          </p>
        </div>

        <div class="flex items-center justify-center gap-1">
          <For each={steps()}>
            {(label, index) => {
              const isActive = () => index() === currentStep()
              const isCompleted = () => index() < currentStep()

              return (
                <div class="flex items-center gap-1">
                  <Show when={index() > 0}>
                    <div class={`h-px w-8 ${isCompleted() ? 'bg-primary' : 'bg-border'}`} />
                  </Show>
                  <Badge
                    variant={isActive() ? 'default' : isCompleted() ? 'secondary' : 'outline'}
                    class="text-xs"
                  >
                    <Show when={isCompleted()} fallback={label}>
                      <HugeIcon icon={Tick02Icon} size={12} class="mr-0.5" />
                      {label}
                    </Show>
                  </Badge>
                </div>
              )
            }}
          </For>
        </div>

        <Show when={formatted() && !isExpired()}>
          <p
            class={`text-center text-xs font-mono ${
              isWarning() ? 'font-semibold text-destructive' : 'text-muted-foreground'
            }`}
          >
            Session expires in {formatted()}
          </p>
        </Show>

        <Card>
          <CardContent>
            <Outlet />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

function SetupError(props: { error: Error }) {
  const navigate = useNavigate()
  const activeInstanceId = useInstanceStore((state) => state.activeInstanceId)
  const instances = useInstanceStore((state) => state.instances)

  const backendUrl = () => {
    const id = activeInstanceId()
    if (!id) return ''
    return instances()[id]?.url ?? ''
  }

  const frontendOrigin = window.location.origin
  const issue = () => {
    const url = backendUrl()
    if (!url) return null
    return getConnectivityIssue(url, props.error, frontendOrigin)
  }
  const hostedUi = isHostedUiOrigin(frontendOrigin)

  return (
    <div class="flex min-h-screen items-center justify-center p-6">
      <div class="w-full max-w-xl space-y-4">
        <Show
          when={issue()}
          fallback={
            <Alert variant="destructive">
              <AlertTitle>Something went wrong</AlertTitle>
              <AlertDescription>{props.error.message}</AlertDescription>
            </Alert>
          }
        >
          <Alert variant="destructive">
            <AlertTitle>{issue()?.title}</AlertTitle>
            <AlertDescription>{issue()?.description}</AlertDescription>
          </Alert>

          <Card>
            <CardContent class="space-y-4 py-5">
              <div class="space-y-1">
                <p class="text-sm font-medium">Use CLI setup</p>
                <p class="text-sm text-muted-foreground">
                  Complete first-run setup directly on the backend host:
                </p>
                <code class="block bg-muted px-2 py-1 text-xs">oore setup</code>
              </div>

              <div class="space-y-1">
                <p class="text-sm font-medium">Expose backend over HTTPS</p>
                <p class="text-sm text-muted-foreground">
                  Use a tunnel and reconnect with the assigned HTTPS URL:
                </p>
                <code class="block bg-muted px-2 py-1 text-xs">
                  cloudflared tunnel --url {backendUrl()}
                </code>
              </div>

              <Show when={hostedUi}>
                <div class="space-y-1">
                  <p class="text-sm font-medium">Use local/self-hosted web UI</p>
                  <p class="text-sm text-muted-foreground">
                    If backend stays local-only, run the bundled local web launcher:
                  </p>
                  <code class="block bg-muted px-2 py-1 text-xs">
                    oore-web --backend-url {backendUrl()}
                  </code>
                </div>
              </Show>
            </CardContent>
          </Card>
        </Show>

        <div class="flex gap-2">
          <Button onClick={() => void navigate({ to: '/setup' })}>Retry Setup</Button>
          <Button
            variant="outline"
            onClick={() => window.open('https://docs.oore.build', '_blank')}
          >
            Open Docs
          </Button>
        </div>
      </div>
    </div>
  )
}
