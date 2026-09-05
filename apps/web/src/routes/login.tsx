import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, Tick02Icon } from '@hugeicons/core-free-icons'
import type { ConnectivityIssue } from '@/lib/connectivity'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { useSetupStatus } from '@/hooks/use-setup'
import { useTrustedProxyAutoLogin } from '@/hooks/use-trusted-proxy-auto-login'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  getSetupStatus,
  localLogin,
  oidcStart,
  trustedProxyLogin,
} from '@oore/client/operations'
import {
  getConnectivityIssue,
  isHostedUiOrigin,
  isLoopbackHostname,
  isMixedContentBlocked,
  resolveUrlHostname,
} from '@/lib/connectivity'
import { Separator } from '@/components/ui/separator'
import { Spinner } from '@/components/ui/spinner'
import { getLastAuthMetaForInstance, useAuthStore } from '@/stores/auth-store'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'
import { PageMeta } from '@/lib/seo'
import { resolveLoginFlow } from '@/lib/login-flow'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'

import DemoLoginForm from '@/components/demo-login-form'
import { isDemoMode } from '@/lib/demo-mode'
import { useTime } from '@/hooks/use-time'
import { createWebOoreClient } from '@/lib/api-client/client'

export const Route = createFileRoute('/login')({
  component: LoginPage,
  staticData: {
    breadcrumb: {
      title: 'Login',
    },
  },
})

const lastAuthTimeFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

const RECOVERY_LINK_ERROR =
  'This recovery link is missing, expired, already used, or for a different account. Run oore recovery on the daemon host to create a new link.'

const LOGIN_ERROR_MESSAGES = {
  local_recovery_capability_required: RECOVERY_LINK_ERROR,
  local_recovery_capability_invalid: RECOVERY_LINK_ERROR,
  local_recovery_account_mismatch: RECOVERY_LINK_ERROR,
  local_login_loopback_required:
    'Local Only sign-in is restricted to loopback access. Finish setup from the daemon host, or switch this instance to Remote with your chosen auth method.',
  mode_restricted:
    'This sign-in method is not enabled for the active instance. Check the setup mode on the daemon host.',
  external_access_https_required:
    'External Access requires an HTTPS public URL.',
  external_access_origin_not_allowed:
    'External Access Public URL origin is not included in allowed frontend origins.',
  external_access_public_url_missing:
    'Set External Access Public URL in Preferences on the host machine before enabling External Access.',
  external_access_preflight_failed:
    'External Access preflight checks are failing. Resolve setup and Preferences readiness checks first.',
  trusted_proxy_peer_not_allowed:
    'Trusted proxy login request did not come from an allowlisted proxy peer.',
  trusted_proxy_identity_missing:
    'Trusted proxy identity header is missing. Check proxy header forwarding.',
  trusted_proxy_identity_invalid:
    'Trusted proxy identity header must contain an email address.',
} satisfies Record<string, string>

function instanceHostname(url: string): string {
  if (!url.trim()) return window.location.host
  try {
    return new URL(url).hostname
  } catch {
    return url || window.location.host
  }
}

function formatLastAuthTime(epochSeconds: number): string {
  return lastAuthTimeFormatter.format(epochSeconds * 1000)
}

function formatAuthMethodLabel(
  method: 'oidc' | 'local' | 'recovery' | 'trusted_proxy',
): string {
  if (method === 'local') return 'Local Only'
  if (method === 'recovery') return 'Local recovery'
  if (method === 'trusted_proxy') return 'Trusted Proxy'
  return 'OIDC'
}

function assertCompleteUserProfile<
  T extends { user_id?: string | null; role?: string | null },
>(
  user: T,
): asserts user is T & { user_id: string; role: NonNullable<T['role']> } {
  if (!user.user_id || !user.role) {
    throw new Error('Incomplete user profile received from server')
  }
}

export function recoveryCapabilityFromHash(hash: string): string | null {
  const value = new URLSearchParams(hash.replace(/^#/, '')).get('recovery')
  return value && /^oore_recovery_[0-9a-f]{64}$/.test(value) ? value : null
}

export function buildLoginBackendCommands(backendUrl: string) {
  const backendUrlArgument = `'${backendUrl.replaceAll("'", `'"'"'`)}'`
  return {
    cloudflared: 'cloudflared tunnel run <tunnel-name>',
    ooreWeb: `oore-web --backend-url ${backendUrlArgument}`,
  }
}

function LoginPage() {
  const instance = useActiveInstance()
  const instances = useInstanceStore((s) => s.instances)
  const activeInstanceId = useInstanceStore((s) => s.activeInstanceId)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)
  const navigate = useNavigate()
  const setAuth = useAuthStore((s) => s.setAuth)
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  const time = useTime()
  const hasValidToken =
    !!token && expiresAt != null && expiresAt > Math.floor(time / 1000)
  const setupStatusQuery = useSetupStatus()
  const [showAddInstance, setShowAddInstance] = useState(false)
  const [loading, setLoading] = useState(false)
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? null
  const [localEmail, setLocalEmail] = useState('')
  const [recoveryCapability, setRecoveryCapability] = useState(() =>
    recoveryCapabilityFromHash(window.location.hash),
  )
  const [error, setError] = useState<string | null>(null)
  const [connectivityIssue, setConnectivityIssue] =
    useState<ConnectivityIssue | null>(null)
  const hostedUi = isHostedUiOrigin(window.location.origin)
  const instanceList = Object.values(instances).sort((a, b) => {
    if (a.id === activeInstanceId) return -1
    if (b.id === activeInstanceId) return 1
    return b.addedAt - a.addedAt
  })
  const lastAuthMeta = instance ? getLastAuthMetaForInstance(instance.id) : null
  const instanceApiBaseUrl = resolveInstanceApiBaseUrl(instance)
  const uiIsLoopback = isLoopbackHostname(window.location.hostname)
  const backendIsLoopback = instanceApiBaseUrl
    ? isLoopbackHostname(resolveUrlHostname(instanceApiBaseUrl))
    : false
  const loopbackLocalPath = uiIsLoopback && backendIsLoopback
  const loginFlow = setupStatusQuery.data
    ? resolveLoginFlow(setupStatusQuery.data, Boolean(recoveryCapability))
    : null
  const localLoginAvailable = loginFlow === 'local' && loopbackLocalPath
  const trustedProxyLoginAvailable = loginFlow === 'trusted_proxy'
  const localModeNetworkBlocked = runtimeMode === 'local' && !loopbackLocalPath

  useMountEffect(() => {
    if (
      new URLSearchParams(window.location.hash.replace(/^#/, '')).has(
        'recovery',
      )
    ) {
      window.history.replaceState(
        null,
        '',
        window.location.pathname + window.location.search,
      )
    }
  })

  useMountEffect(() => {
    if (hasValidToken) {
      void navigate({ to: '/' })
    }
  })

  useMountEffect(() => {
    let prevId = useInstanceStore.getState().activeInstanceId
    const unsub = useInstanceStore.subscribe((state) => {
      if (state.activeInstanceId !== prevId) {
        prevId = state.activeInstanceId
        setError(null)
        setConnectivityIssue(null)
      }
    })
    return unsub
  })

  async function handleLogin() {
    if (!instance) return
    const baseUrl = resolveInstanceApiBaseUrl(instance)
    if (!baseUrl) return
    const client = createWebOoreClient({ baseUrl })
    setLoading(true)
    setError(null)
    setConnectivityIssue(null)

    if (isMixedContentBlocked(window.location.origin, baseUrl)) {
      setConnectivityIssue(
        getConnectivityIssue(
          baseUrl,
          new Error('mixed_content_blocked'),
          window.location.origin,
        ),
      )
      setError('Browser blocked this request due to mixed-content policy.')
      setLoading(false)
      return
    }

    try {
      const status = await getSetupStatus({ client })
      if (status.setup_mode && status.runtime_mode !== 'local') {
        setLoading(false)
        void navigate({ to: '/setup' })
        return
      }
      const localUi = isLoopbackHostname(window.location.hostname)
      const localBackend = isLoopbackHostname(resolveUrlHostname(baseUrl))
      const canUseLoopbackLocalLogin = localUi && localBackend
      if (status.runtime_mode === 'local' && !canUseLoopbackLocalLogin) {
        setError(
          'Local Only sign-in is restricted to loopback access. Finish setup from the daemon host, or switch this instance to Remote with your chosen auth method.',
        )
        setLoading(false)
        return
      }

      const resolvedLoginFlow = resolveLoginFlow(
        status,
        Boolean(recoveryCapability),
      )

      if (resolvedLoginFlow === 'local' || resolvedLoginFlow === 'recovery') {
        const capability =
          resolvedLoginFlow === 'recovery' ? recoveryCapability : null
        setRecoveryCapability(null)
        const response = await localLogin({
          body: {
            email:
              resolvedLoginFlow === 'local'
                ? localEmail.trim() || undefined
                : undefined,
            recovery_capability: capability ?? undefined,
          },
          client,
        })
        assertCompleteUserProfile(response.user)
        setAuth(
          response.session_token,
          response.expires_at,
          {
            email: response.user.email,
            oidc_subject: response.user.oidc_subject,
            user_id: response.user.user_id,
            role: response.user.role,
            avatar_url: response.user.avatar_url ?? undefined,
          },
          resolvedLoginFlow,
        )
        setLoading(false)
        void navigate({ to: '/' })
        return
      }

      if (resolvedLoginFlow === 'trusted_proxy') {
        const response = await trustedProxyLogin({ client })
        assertCompleteUserProfile(response.user)
        setAuth(
          response.session_token,
          response.expires_at,
          {
            email: response.user.email,
            oidc_subject: response.user.oidc_subject,
            user_id: response.user.user_id,
            role: response.user.role,
            avatar_url: response.user.avatar_url ?? undefined,
          },
          'trusted_proxy',
        )
        setLoading(false)
        void navigate({ to: '/' })
        return
      }

      const callbackUrl = `${window.location.origin}/auth/callback`
      const data = await oidcStart({
        client,
        query: { redirect_uri: callbackUrl },
      })

      try {
        sessionStorage.setItem('oore_oidc_state', data.state)
        sessionStorage.setItem('oore_oidc_instance', instance.id)
        sessionStorage.setItem('oore_oidc_flow', 'auth')
      } catch {
        // sessionStorage unavailable
      }

      window.location.href = data.authorization_url
    } catch (e) {
      setConnectivityIssue(
        getConnectivityIssue(baseUrl, e, window.location.origin),
      )
      setError(getApiErrorMessage(e, LOGIN_ERROR_MESSAGES))
    }
    setLoading(false)
  }

  useTrustedProxyAutoLogin({
    enabled:
      !!instance &&
      !hasValidToken &&
      !loading &&
      setupStatusQuery.data?.is_configured === true &&
      trustedProxyLoginAvailable,
    instanceId: instance?.id ?? null,
    onLogin: handleLogin,
  })

  const backendCommands = buildLoginBackendCommands(instance?.url ?? '')

  if (isDemoMode) {
    return (
      <div className="focused-flow flex min-h-0 flex-1 flex-col items-center p-4 sm:p-6">
        <PageMeta title="Demo login" />
        <div className="w-full max-w-sm space-y-8">
          <div className="space-y-4 text-center">
            <div className="mx-auto flex size-14 items-center justify-center">
              <img src="/logo.svg" alt="Oore logo" className="size-full" />
            </div>
            <div className="space-y-1">
              <h1 className="text-3xl font-bold tracking-tight">
                Explore the Oore demo
              </h1>
              <p className="text-sm text-muted-foreground">
                Choose a role to see its real navigation, data, and permissions.
              </p>
            </div>
          </div>
          <DemoLoginForm />
        </div>
      </div>
    )
  }

  return (
    <div className="focused-flow flex min-h-0 flex-1 flex-col items-center p-4 sm:p-6">
      <PageMeta title="Login" />
      <div className="w-full max-w-sm space-y-8">
        <div className="space-y-4 text-center">
          <div className="mx-auto flex size-14 items-center justify-center">
            <img src="/logo.svg" alt="Oore logo" className="size-full" />
          </div>
          <div className="space-y-1">
            <h1 className="text-3xl font-bold tracking-tight">Sign in</h1>
            <p className="text-sm text-muted-foreground">
              Authenticate to the active instance to continue.
            </p>
          </div>
        </div>

        <Card size="sm">
          <CardContent className="space-y-4">
            <div className="flex items-center gap-2 text-sm">
              <span className="text-muted-foreground">Instance</span>
              <Separator orientation="vertical" className="h-3!" />
              <code className="bg-muted px-1.5 py-0.5 font-mono text-xs font-medium">
                {instance?.label ?? 'none selected'}
              </code>
            </div>

            <Item variant="muted" size="sm">
              <ItemContent>
                <ItemDescription>Sign-in method</ItemDescription>
                <ItemTitle>
                  {loginFlow === 'local'
                    ? runtimeMode === 'local'
                      ? 'Local Only'
                      : 'Local (loopback)'
                    : loginFlow === 'recovery'
                      ? 'Local recovery'
                      : loginFlow === 'trusted_proxy'
                        ? 'Trusted Proxy'
                        : loginFlow === 'oidc'
                          ? 'OIDC'
                          : 'Checking...'}
                </ItemTitle>
                <ItemDescription>
                  {lastAuthMeta
                    ? `Last successful sign-in: ${formatLastAuthTime(lastAuthMeta.at)} via ${formatAuthMethodLabel(lastAuthMeta.method)}`
                    : 'No previous successful sign-in stored on this device.'}
                </ItemDescription>
              </ItemContent>
            </Item>

            {runtimeMode === 'local' && localModeNetworkBlocked ? (
              <Alert variant="destructive">
                <AlertDescription>
                  Local Only sign-in is blocked for this network host. Finish
                  setup from the daemon host, or switch this instance to Remote
                  with your chosen auth method.
                </AlertDescription>
              </Alert>
            ) : null}

            {trustedProxyLoginAvailable ? (
              <Alert>
                <AlertDescription>
                  Your upstream proxy has already authenticated this request.
                  Continue to create an Oore session from the forwarded
                  identity.
                </AlertDescription>
              </Alert>
            ) : null}

            {loginFlow === 'recovery' ? (
              <Alert>
                <AlertDescription>
                  Continue with the single-use recovery link minted on the
                  daemon host. Its capability has already been removed from the
                  address bar and will be sent only in this sign-in request.
                </AlertDescription>
              </Alert>
            ) : null}

            {localLoginAvailable && !localModeNetworkBlocked ? (
              <div className="flex flex-col gap-2">
                <Label htmlFor="local-login-email">Email (optional)</Label>
                <Input
                  id="local-login-email"
                  type="email"
                  autoComplete="email"
                  placeholder="you@example.com"
                  value={localEmail}
                  onChange={(event) => setLocalEmail(event.target.value)}
                  disabled={loading}
                />
                <p className="text-xs text-muted-foreground">
                  Leave email blank to auto-sign-in when only one active user
                  exists.
                </p>
                {runtimeMode === 'local' ? (
                  <p className="text-xs text-muted-foreground">
                    First sign-in on a new local instance will auto-initialize
                    owner setup.
                  </p>
                ) : null}
              </div>
            ) : null}

            {error ? (
              <Alert variant="destructive">
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            ) : null}

            {connectivityIssue && instance ? (
              <Alert>
                <AlertTitle>{connectivityIssue.title}</AlertTitle>
                <AlertDescription className="space-y-3">
                  <p>{connectivityIssue.description}</p>

                  <div className="space-y-1">
                    <p className="text-xs font-medium text-foreground">
                      CLI fallback
                    </p>
                    <code className="block rounded-md bg-muted px-2 py-1 text-xs text-foreground">
                      oore setup
                    </code>
                  </div>

                  <div className="space-y-1">
                    <p className="text-xs font-medium text-foreground">
                      Publish through protected ingress
                    </p>
                    <p>
                      Use a named tunnel that Cloudflare Access protects. Do not
                      use a public Quick Tunnel for Oore.
                    </p>
                    <code className="block rounded-md bg-muted px-2 py-1 text-xs text-foreground">
                      {backendCommands.cloudflared}
                    </code>
                    <a
                      href="https://docs.oore.build/operate/access/cloudflare-access"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-block text-xs text-foreground underline underline-offset-2"
                    >
                      Open the Cloudflare Access guide
                    </a>
                  </div>

                  {hostedUi ? (
                    <p>
                      For local-only backends, run the bundled local web
                      launcher: <code>{backendCommands.ooreWeb}</code>.
                    </p>
                  ) : null}
                </AlertDescription>
              </Alert>
            ) : null}

            <Button
              onClick={() => void handleLogin()}
              disabled={
                loading ||
                !instance ||
                localModeNetworkBlocked ||
                setupStatusQuery.isLoading
              }
              className="w-full"
            >
              {loading ? (
                <>
                  <Spinner className="size-4" />
                  {loginFlow === 'oidc' ? 'Redirecting...' : 'Signing in...'}
                </>
              ) : loginFlow === 'local' ? (
                localModeNetworkBlocked ? (
                  'Unavailable from this host'
                ) : (
                  'Sign in locally'
                )
              ) : loginFlow === 'recovery' ? (
                'Recover local access'
              ) : trustedProxyLoginAvailable ? (
                'Continue with trusted proxy'
              ) : setupStatusQuery.isLoading ? (
                'Checking sign-in method...'
              ) : (
                'Sign in with OIDC'
              )}
            </Button>
          </CardContent>
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle>Saved instances</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {instanceList.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No saved instances yet. Add one to start signing in.
              </p>
            ) : (
              <RadioGroup
                aria-label="Saved instances"
                value={activeInstanceId ?? ''}
                onValueChange={setActiveInstance}
              >
                {instanceList.map((inst) => {
                  const isActive = inst.id === activeInstanceId
                  const meta = getLastAuthMetaForInstance(inst.id)
                  return (
                    <Item
                      key={inst.id}
                      variant="outline"
                      render={
                        <label
                          htmlFor={`login-instance-${inst.id}`}
                          className="cursor-pointer"
                        />
                      }
                      className="items-start has-data-checked:border-primary has-data-checked:bg-accent"
                    >
                      <RadioGroupItem
                        id={`login-instance-${inst.id}`}
                        value={inst.id}
                        className="mt-0.5"
                      />
                      <ItemContent>
                        <ItemTitle>{inst.label}</ItemTitle>
                        <ItemDescription className="line-clamp-none">
                          {instanceHostname(inst.url)}
                          <span className="mt-1 block">
                            {meta
                              ? `Last sign-in: ${formatLastAuthTime(meta.at)} via ${formatAuthMethodLabel(meta.method)}`
                              : 'No successful sign-in stored for this instance'}
                          </span>
                        </ItemDescription>
                      </ItemContent>
                      <ItemActions>
                        {isActive ? (
                          <span className="flex items-center gap-1 text-xs text-foreground">
                            <HugeiconsIcon icon={Tick02Icon} size={14} />
                            Active
                          </span>
                        ) : null}
                      </ItemActions>
                    </Item>
                  )
                })}
              </RadioGroup>
            )}
            <Button
              variant="outline"
              className="w-full"
              onClick={() => setShowAddInstance(true)}
            >
              <HugeiconsIcon icon={Add01Icon} />
              Add another instance
            </Button>
          </CardContent>
        </Card>
      </div>
      <AddInstanceDialog
        open={showAddInstance}
        onOpenChange={setShowAddInstance}
      />
    </div>
  )
}
