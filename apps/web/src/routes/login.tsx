import { createEffect, createMemo, createSignal, For, Show } from 'solid-js'
import { createForm } from '@tanstack/solid-form'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import { Add01Icon, ArrowRight01Icon, Login02Icon } from '@hugeicons/core-free-icons'
import z from 'zod'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import { HugeIcon } from '@/components/huge-icon'
import InstanceSwitcher from '@/components/InstanceSwitcher'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { Spinner } from '@/components/ui/spinner'
import {
  ApiClientError,
  getSetupStatus,
  localLogin,
  trustedProxyLogin,
} from '@/lib/api'
import { resolveLoginFlow } from '@/lib/login-flow'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

function isLoopbackHostname(hostname: string): boolean {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '::1' ||
    hostname === '[::1]'
  )
}

function resolveBackendHostname(url: string): string {
  const trimmed = url.trim()
  if (!trimmed) return window.location.hostname
  try {
    return new URL(trimmed).hostname
  } catch {
    return ''
  }
}

function LoginPage() {
  const navigate = useNavigate()
  const instance = useActiveInstance()

  const instances = useInstanceStore((state) => state.instances)
  const activeInstanceId = useInstanceStore((state) => state.activeInstanceId)
  const setActiveInstance = useInstanceStore((state) => state.setActiveInstance)

  const setAuth = useAuthStore((state) => state.setAuth)
  const token = useAuthStore((state) => state.token)
  const expiresAt = useAuthStore((state) => state.expiresAt)

  const hasValidToken = createMemo(() => {
    return (
      !!token() &&
      expiresAt() != null &&
      (expiresAt() as number) > Math.floor(Date.now() / 1000)
    )
  })

  const [showAddInstance, setShowAddInstance] = createSignal(false)
  const [loading, setLoading] = createSignal(false)
  const [runtimeMode, setRuntimeMode] = createSignal<'local' | 'remote' | null>(
    null,
  )
  const [remoteAuthMode, setRemoteAuthMode] = createSignal<
    'oidc' | 'trusted_proxy' | null
  >(null)
  const [error, setError] = createSignal<string | null>(null)

  const localEmailSchema = z
    .string()
    .trim()
    .email('Local user email must be valid.')
    .or(z.literal(''))

  const loginForm = createForm(() => ({
    defaultValues: {
      localEmail: '',
    },
    validators: {
      onSubmit: ({ value }) => {
        const parsed = z
          .object({
            localEmail: localEmailSchema,
          })
          .safeParse(value)
        if (parsed.success) return undefined
        return {
          fields: {
            localEmail:
              parsed.error.flatten().fieldErrors.localEmail?.[0] ??
              'Local user email must be valid.',
          },
        }
      },
    },
    onSubmit: ({ value }) => {
      void handleLogin(value.localEmail)
    },
  }))

  const loginSubmissionAttempts = loginForm.useStore(
    (state) => state.submissionAttempts,
  )

  const instanceList = createMemo(() =>
    Object.values(instances()).sort((left, right) => {
      if (left.id === activeInstanceId()) return -1
      if (right.id === activeInstanceId()) return 1
      return right.addedAt - left.addedAt
    }),
  )

  createEffect(() => {
    if (hasValidToken()) {
      void navigate({ to: '/' })
    }
  })

  createEffect(() => {
    const activeInstance = instance()
    if (!activeInstance) {
      setRuntimeMode(null)
      setRemoteAuthMode(null)
      return
    }

    void getSetupStatus(activeInstance.url)
      .then((status) => {
        setRuntimeMode(status.runtime_mode)
        setRemoteAuthMode(status.remote_auth_mode)
      })
      .catch(() => {
        setRuntimeMode(null)
        setRemoteAuthMode(null)
      })
  })

  const handleLogin = async (localEmail: string) => {
    const activeInstance = instance()
    if (!activeInstance) return

    setLoading(true)
    setError(null)

    try {
      const status = await getSetupStatus(activeInstance.url)
      if (status.setup_mode && status.runtime_mode !== 'local') {
        setError('Setup is not complete yet. Finish setup before signing in.')
        setLoading(false)
        return
      }

      setRuntimeMode(status.runtime_mode)
      setRemoteAuthMode(status.remote_auth_mode)

      const localUi = isLoopbackHostname(window.location.hostname)
      const localBackend = isLoopbackHostname(
        resolveBackendHostname(activeInstance.url),
      )
      const canUseLoopbackLocalLogin = localUi && localBackend
      const loginFlow = resolveLoginFlow(status, canUseLoopbackLocalLogin)

      if (loginFlow === 'local') {
        const response = await localLogin(activeInstance.url, {
          email: localEmail.trim() || undefined,
        })
        if (!response.user.user_id || !response.user.role) {
          throw new Error('Incomplete user profile received from server')
        }

        setAuth()(
          response.session_token,
          response.expires_at,
          {
            email: response.user.email,
            oidc_subject: response.user.oidc_subject,
            user_id: response.user.user_id,
            role: response.user.role,
            avatar_url: response.user.avatar_url,
          },
          'local',
        )

        setLoading(false)
        void navigate({ to: '/' })
        return
      }

      if (loginFlow === 'trusted_proxy') {
        const response = await trustedProxyLogin(activeInstance.url)
        if (!response.user.user_id || !response.user.role) {
          throw new Error('Incomplete user profile received from server')
        }

        setAuth()(
          response.session_token,
          response.expires_at,
          {
            email: response.user.email,
            oidc_subject: response.user.oidc_subject,
            user_id: response.user.user_id,
            role: response.user.role,
            avatar_url: response.user.avatar_url,
          },
          'trusted_proxy',
        )

        setLoading(false)
        void navigate({ to: '/' })
        return
      }

      const callbackUrl = `${window.location.origin}/auth/callback`
      const response = await fetch(
        `${activeInstance.url}/v1/auth/oidc/start?redirect_uri=${encodeURIComponent(callbackUrl)}`,
      )

      if (!response.ok) {
        const body = (await response.json().catch(() => ({}))) as {
          error?: string
        }
        throw new Error(body.error ?? `Login failed (${response.status})`)
      }

      const data = (await response.json()) as {
        authorization_url: string
        state: string
      }

      try {
        sessionStorage.setItem('oore_oidc_state', data.state)
        sessionStorage.setItem('oore_oidc_instance', activeInstance.id)
        sessionStorage.setItem('oore_oidc_flow', 'auth')
      } catch {
        // sessionStorage unavailable
      }

      window.location.href = data.authorization_url
    } catch (value) {
      if (value instanceof ApiClientError) {
        setError(value.message)
      } else {
        setError(value instanceof Error ? value.message : 'Login failed')
      }
      setLoading(false)
    }
  }

  return (
    <div class="mx-auto max-w-6xl p-6">
      <PageMeta title="Login" />
      <div class="mb-6 text-center">
        <h1 class="text-3xl font-semibold tracking-tight">Sign in to Oore CI</h1>
        <p class="mt-2 text-sm text-muted-foreground">
          Choose an instance and authenticate.
        </p>
      </div>

      <div class="grid gap-4 lg:grid-cols-[1fr_340px]">
        <Card>
          <CardHeader>
            <CardTitle>Authentication</CardTitle>
          </CardHeader>
          <CardContent class="space-y-4">
            <Show
              when={instance()}
              fallback={
                <div class="space-y-3">
                  <p class="text-sm text-muted-foreground">
                    Add a backend instance first.
                  </p>
                  <Button onClick={() => setShowAddInstance(true)}>
                    <HugeIcon icon={Add01Icon} />
                    Add Instance
                  </Button>
                </div>
              }
            >
              <div class="rounded border bg-muted/30 p-3 text-xs text-muted-foreground">
                Active instance: <strong>{instance()?.label}</strong> ({instance()?.url})
              </div>

              <Show when={runtimeMode() === 'local'}>
                <loginForm.Field name="localEmail">
                  {(field) => {
                    const fieldError = () => {
                      if (
                        !field().state.meta.isTouched &&
                        loginSubmissionAttempts() === 0
                      ) {
                        return null
                      }
                      return (field().state.meta.errors[0] as string | undefined) ?? null
                    }

                    return (
                      <FormField
                        label="Local user email (optional)"
                        error={fieldError()}
                      >
                        <Input
                          value={field().state.value}
                          onInput={(event) =>
                            field().handleChange(event.currentTarget.value)
                          }
                          onBlur={field().handleBlur}
                          placeholder="owner@example.com"
                        />
                      </FormField>
                    )
                  }}
                </loginForm.Field>
              </Show>

              <Show when={error()}>
                <Alert variant="destructive">
                  <AlertTitle>Sign in failed</AlertTitle>
                  <AlertDescription>{error()}</AlertDescription>
                </Alert>
              </Show>

              <Button onClick={() => void loginForm.handleSubmit()} disabled={loading()}>
                <Show when={loading()} fallback={<HugeIcon icon={Login02Icon} />}>
                  <Spinner class="size-4" />
                </Show>
                Continue
                <HugeIcon icon={ArrowRight01Icon} size={14} />
              </Button>

              <div class="text-xs text-muted-foreground">
                Flow: {runtimeMode() ?? 'unknown'} / {remoteAuthMode() ?? 'unknown'}
              </div>
            </Show>

            <Separator />

            <div class="space-y-2">
              <h3 class="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Instance quick switch
              </h3>
              <div class="grid gap-2">
                <For each={instanceList()}>
                  {(item) => (
                    <button
                      type="button"
                      onClick={() => setActiveInstance()(item.id)}
                      class={`border px-3 py-2 text-left text-xs ${
                        item.id === activeInstanceId()
                          ? 'border-primary bg-primary/10'
                          : 'border-border hover:bg-accent'
                      }`}
                    >
                      <div class="font-medium">{item.label}</div>
                      <div class="text-muted-foreground">{item.url}</div>
                    </button>
                  )}
                </For>
              </div>
            </div>
          </CardContent>
        </Card>

        <InstanceSwitcher />
      </div>

      <AddInstanceDialog open={showAddInstance()} onOpenChange={setShowAddInstance} />
    </div>
  )
}
