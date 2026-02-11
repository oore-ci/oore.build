import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, Tick02Icon } from '@hugeicons/core-free-icons'
import { Alert, AlertDescription } from '@/components/ui/alert'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { Spinner } from '@/components/ui/spinner'
import { getLastAuthMetaForInstance, useAuthStore } from '@/stores/auth-store'
import { useActiveInstance, useInstanceStore  } from '@/stores/instance-store'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

function instanceHostname(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return url || 'local'
  }
}

function formatLastAuthTime(epochSeconds: number): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(epochSeconds * 1000)
}

function LoginPage() {
  const instance = useActiveInstance()
  const instances = useInstanceStore((s) => s.instances)
  const activeInstanceId = useInstanceStore((s) => s.activeInstanceId)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)
  const navigate = useNavigate()
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  const hasValidToken =
    !!token && expiresAt != null && expiresAt > Math.floor(Date.now() / 1000)
  const [showAddInstance, setShowAddInstance] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
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

  useEffect(() => {
    document.title = webPageTitle('Login')
  }, [])

  useEffect(() => {
    if (hasValidToken) {
      void navigate({ to: '/' })
    }
  }, [hasValidToken, navigate])

  useEffect(() => {
    setError(null)
  }, [instance?.id])

  const handleLogin = async () => {
    if (!instance) return
    setLoading(true)
    setError(null)

    try {
      const callbackUrl = `${window.location.origin}/auth/callback`
      const res = await fetch(
        `${instance.url}/v1/auth/oidc/start?redirect_uri=${encodeURIComponent(callbackUrl)}`,
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
      } catch {
        // sessionStorage unavailable
      }

      window.location.href = data.authorization_url
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Login failed')
      setLoading(false)
    }
  }

  return (
    <div className="flex-1 flex flex-col items-center justify-center p-6">
      <div className="w-full max-w-sm space-y-8">
        <div className="text-center space-y-4">
          <div className="mx-auto flex size-14 items-center justify-center">
            <img src="/logo.svg" alt="Oore logo" className="size-full" />
          </div>
          <div className="space-y-1">
            <h1 className="text-3xl font-bold tracking-tight">Sign in</h1>
            <p className="text-muted-foreground text-sm">
              Authenticate with your identity provider to continue.
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
              <p className="mt-1 text-sm font-medium">OIDC</p>
              <p className="mt-1 text-xs text-muted-foreground">
                {lastAuthMeta
                  ? `Last successful sign-in: ${formatLastAuthTime(lastAuthMeta.at)}`
                  : 'No previous successful sign-in stored on this device.'}
              </p>
            </div>

            {error ? (
              <Alert variant="destructive">
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            ) : null}

            <Button
              onClick={() => void handleLogin()}
              disabled={loading || !instance}
              className="w-full"
            >
              {loading ? (
                <>
                  <Spinner className="size-4" />
                  Redirecting...
                </>
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
                            ? `Last sign-in: ${formatLastAuthTime(meta.at)} via OIDC`
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
              <HugeiconsIcon icon={Add01Icon} size={16} />
              Add Another Instance
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
