import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useCallback, useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, Tick02Icon } from '@hugeicons/core-free-icons'
import type { ConnectivityIssue } from '@/lib/connectivity'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { useSetupStatus } from '@/hooks/use-setup'
import { useTrustedProxyAutoLogin } from '@/hooks/use-trusted-proxy-auto-login'
import { Alert, AlertDescription } from '@/components/ui/alert'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import {
  ApiClientError,
  getSetupStatus,
  localLogin,
  trustedProxyLogin,
} from '@/lib/api'
import {
  getConnectivityIssue,
  isHostedUiOrigin,
  isMixedContentBlocked,
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

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

const lastAuthTimeFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

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
  method: 'oidc' | 'local' | 'trusted_proxy',
): string {
  if (method === 'local') return 'Local Only'
  if (method === 'trusted_proxy') return 'Trusted Proxy'
  return 'OIDC'
}

export function buildLoginBackendCommands(backendUrl: string) {
  const backendUrlArgument = `'${backendUrl.replaceAll("'", `'"'"'`)}'`
  return {
    cloudflared: `cloudflared tunnel --url ${backendUrlArgument}`,
    ooreWeb: `oore-web --backend-url ${backendUrlArgument}`,
  }
}

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

function useLoginPageState() {
  const instance = useActiveInstance()
  const instances = useInstanceStore((s) => s.instances)
  const activeInstanceId = useInstanceStore((s) => s.activeInstanceId)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)
  const navigate = useNavigate()
  const setAuth = useAuthStore((s) => s.setAuth)
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  const hasValidToken =
    !!token && expiresAt != null && expiresAt > Math.floor(Date.now() / 1000)
  const setupStatusQuery = useSetupStatus()
  const [showAddInstance, setShowAddInstance] = useState(false)
  const [loading, setLoading] = useState(false)
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? null
  const [localEmail, setLocalEmail] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [connectivityIssue, setConnectivityIssue] =
    useState<ConnectivityIssue | null>(null)
  const hostedUi = isHostedUiOrigin(window.location.origin)
  const instanceList = useMemo(
    () =>
      Object.values(instances).sort((a, b) => {
        if (a.id === activeInstanceId) return -1
        if (b.id === activeInstanceId) return 1
        return b.addedAt - a.addedAt
      }),
    [instances, activeInstanceId],
  )
  const lastAuthMeta = instance ? getLastAuthMetaForInstance(instance.id) : null
  const instanceApiBaseUrl = resolveInstanceApiBaseUrl(instance)
  const uiIsLoopback = isLoopbackHostname(window.location.hostname)
  const backendIsLoopback = instanceApiBaseUrl
    ? isLoopbackHostname(resolveBackendHostname(instanceApiBaseUrl))
    : false
  const loopbackLocalPath = uiIsLoopback && backendIsLoopback
  const loginFlow = setupStatusQuery.data
    ? resolveLoginFlow(setupStatusQuery.data, loopbackLocalPath)
    : null
  const localLoginAvailable = loginFlow === 'local' && loopbackLocalPath
  const trustedProxyLoginAvailable = loginFlow === 'trusted_proxy'
  const localModeNetworkBlocked = runtimeMode === 'local' && !loopbackLocalPath

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

  const handleLogin = useCallback(async () => {
    if (!instance) return
    const baseUrl = resolveInstanceApiBaseUrl(instance)
    if (!baseUrl) return
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
      const status = await getSetupStatus(baseUrl)
      if (status.setup_mode && status.runtime_mode !== 'local') {
        void navigate({ to: '/setup' })
        setLoading(false)
        return
      }
      const localUi = isLoopbackHostname(window.location.hostname)
      const localBackend = isLoopbackHostname(resolveBackendHostname(baseUrl))
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
        canUseLoopbackLocalLogin,
      )

      if (resolvedLoginFlow === 'local') {
        const response = await localLogin(baseUrl, {
          email: localEmail.trim() || undefined,
        })
        if (!response.user.user_id || !response.user.role) {
          throw new Error('Incomplete user profile received from server')
        }
        setAuth(
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

      if (resolvedLoginFlow === 'trusted_proxy') {
        const response = await trustedProxyLogin(baseUrl)
        if (!response.user.user_id || !response.user.role) {
          throw new Error('Incomplete user profile received from server')
        }
        setAuth(
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
      const res = await fetch(
        `${baseUrl}/v1/auth/oidc/start?redirect_uri=${encodeURIComponent(callbackUrl)}`,
      )

      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as {
          error?: string
        }
        throw new Error(body.error ?? `Login failed (${res.status})`)
      }

      const data = (await res.json()) as {
        authorization_url: string
        state: string
      }

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
      if (e instanceof ApiClientError) {
        if (e.code === 'local_login_loopback_required') {
          setError(
            'Local Only sign-in is restricted to loopback access. Finish setup from the daemon host, or switch this instance to Remote with your chosen auth method.',
          )
        } else if (e.code === 'mode_restricted') {
          setError(
            'This sign-in method is not enabled for the active instance. Check the setup mode on the daemon host.',
          )
        } else if (e.code === 'external_access_https_required') {
          setError('External Access requires an HTTPS public URL.')
        } else if (e.code === 'external_access_origin_not_allowed') {
          setError(
            'External Access Public URL origin is not included in allowed frontend origins.',
          )
        } else if (e.code === 'external_access_public_url_missing') {
          setError(
            'Set External Access Public URL in Preferences on the host machine before enabling External Access.',
          )
        } else if (e.code === 'external_access_preflight_failed') {
          setError(
            'External Access preflight checks are failing. Resolve setup and Preferences readiness checks first.',
          )
        } else if (e.code === 'trusted_proxy_peer_not_allowed') {
          setError(
            'Trusted proxy login request did not come from an allowlisted proxy peer.',
          )
        } else if (e.code === 'trusted_proxy_identity_missing') {
          setError(
            'Trusted proxy identity header is missing. Check proxy header forwarding.',
          )
        } else if (e.code === 'trusted_proxy_identity_invalid') {
          setError(
            'Trusted proxy identity header must contain an email address.',
          )
        } else {
          setError(e.message)
        }
      } else {
        setError(e instanceof Error ? e.message : 'Login failed')
      }
      setLoading(false)
    }
  }, [instance, localEmail, navigate, setAuth])

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

  return {
    activeInstanceId,
    connectivityIssue,
    error,
    handleLogin,
    hostedUi,
    instance,
    instanceList,
    lastAuthMeta,
    loading,
    localEmail,
    localLoginAvailable,
    localModeNetworkBlocked,
    loginFlow,
    runtimeMode,
    setActiveInstance,
    setLocalEmail,
    setShowAddInstance,
    setupStatusQuery,
    showAddInstance,
    trustedProxyLoginAvailable,
  }
}

function LoginPage() {
  const {
    activeInstanceId,
    connectivityIssue,
    error,
    handleLogin,
    hostedUi,
    instance,
    instanceList,
    lastAuthMeta,
    loading,
    localEmail,
    localLoginAvailable,
    localModeNetworkBlocked,
    loginFlow,
    runtimeMode,
    setActiveInstance,
    setLocalEmail,
    setShowAddInstance,
    setupStatusQuery,
    showAddInstance,
    trustedProxyLoginAvailable,
  } = useLoginPageState()
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
        <div className="text-center space-y-4">
          <div className="mx-auto flex size-14 items-center justify-center">
            <img src="/logo.svg" alt="Oore logo" className="size-full" />
          </div>
          <div className="space-y-1">
            <h1 className="text-3xl font-bold tracking-tight">Sign in</h1>
            <p className="text-muted-foreground text-sm">
              Authenticate to the active instance to continue.
            </p>
          </div>
        </div>

        <Card>
          <CardContent className="space-y-4">
            <div className="flex items-center gap-2 text-sm">
              <span className="text-muted-foreground">Instance</span>
              <Separator orientation="vertical" className="h-3!" />
              <code className="bg-muted px-1.5 py-0.5 text-xs font-mono font-medium">
                {instance?.label ?? 'none selected'}
              </code>
            </div>

            <div className="border border-border/60 bg-muted/20 p-3">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Sign-in method
              </p>
              <p className="mt-1 text-sm font-medium">
                {loginFlow === 'local'
                  ? runtimeMode === 'local'
                    ? 'Local Only'
                    : 'Local (loopback)'
                  : loginFlow === 'trusted_proxy'
                    ? 'Trusted Proxy'
                    : loginFlow === 'oidc'
                      ? 'OIDC'
                      : 'Checking...'}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                {lastAuthMeta
                  ? `Last successful sign-in: ${formatLastAuthTime(lastAuthMeta.at)} via ${formatAuthMethodLabel(lastAuthMeta.method)}`
                  : 'No previous successful sign-in stored on this device.'}
              </p>
            </div>

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

            {localLoginAvailable && !localModeNetworkBlocked ? (
              <div className="space-y-2">
                <Input
                  placeholder="Email (optional for single-user instances)"
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
              <div className="space-y-3 border p-3">
                <p className="text-sm font-medium">{connectivityIssue.title}</p>
                <p className="text-xs text-muted-foreground">
                  {connectivityIssue.description}
                </p>

                <div className="space-y-1">
                  <p className="text-xs font-medium">CLI fallback</p>
                  <code className="block bg-muted px-2 py-1 text-xs">
                    oore setup
                  </code>
                </div>

                <div className="space-y-1">
                  <p className="text-xs font-medium">
                    Expose backend with tunnel
                  </p>
                  <code className="block bg-muted px-2 py-1 text-xs">
                    {backendCommands.cloudflared}
                  </code>
                </div>

                {hostedUi ? (
                  <p className="text-xs text-muted-foreground">
                    For local-only backends, run the bundled local web launcher:{' '}
                    <code>{backendCommands.ooreWeb}</code>.
                  </p>
                ) : null}
              </div>
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

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Saved Instances
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {instanceList.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No saved instances yet. Add one to start signing in.
              </p>
            ) : (
              instanceList.map((inst) => {
                const isActive = inst.id === activeInstanceId
                const meta = getLastAuthMetaForInstance(inst.id)
                return (
                  <button
                    key={inst.id}
                    type="button"
                    onClick={() => setActiveInstance(inst.id)}
                    className={`w-full border p-3 text-left transition-colors ${
                      isActive
                        ? 'border-primary/40 bg-primary/5'
                        : 'border-border/60 bg-background hover:border-primary/30'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium">
                          {inst.label}
                        </p>
                        <p className="truncate text-xs text-muted-foreground">
                          {instanceHostname(inst.url)}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {meta
                            ? `Last sign-in: ${formatLastAuthTime(meta.at)} via ${formatAuthMethodLabel(meta.method)}`
                            : 'No successful sign-in stored for this instance'}
                        </p>
                      </div>
                      {isActive ? (
                        <span className="flex items-center gap-1 text-xs text-primary">
                          <HugeiconsIcon icon={Tick02Icon} size={14} />
                          Active
                        </span>
                      ) : null}
                    </div>
                  </button>
                )
              })
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
