import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { Spinner } from '@/components/ui/spinner'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'
import { webPageTitle } from '@/lib/seo'

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

function LoginPage() {
  const instance = useActiveInstance()
  const navigate = useNavigate()
  const token = useAuthStore((s) => s.token)
  const expiresAt = useAuthStore((s) => s.expiresAt)
  const hasValidToken =
    !!token && expiresAt != null && expiresAt > Math.floor(Date.now() / 1000)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    document.title = webPageTitle('Login')
  }, [])

  useEffect(() => {
    if (hasValidToken) {
      void navigate({ to: '/' })
    }
  }, [hasValidToken, navigate])

  useEffect(() => {
    if (!instance) {
      void navigate({ to: '/' })
    }
  }, [instance, navigate])

  if (!instance) {
    return null
  }

  const handleLogin = async () => {
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
                {instance.label}
              </code>
            </div>

            {error ? (
              <Alert variant="destructive">
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            ) : null}

            <Button
              onClick={() => void handleLogin()}
              disabled={loading}
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
      </div>
    </div>
  )
}
