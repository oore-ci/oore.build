import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Spinner } from '@/components/ui/spinner'
import { useActiveInstance } from '@/stores/instance-store'
import { useAuthStore } from '@/stores/auth-store'

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
    document.title = 'Login - oore.build'
  }, [])

  // Already authenticated — redirect to dashboard
  useEffect(() => {
    if (hasValidToken) {
      void navigate({ to: '/' })
    }
  }, [hasValidToken, navigate])

  // No active instance — redirect to home
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

      // Store the OIDC state for callback validation
      try {
        sessionStorage.setItem('oore_oidc_state', data.state)
        sessionStorage.setItem('oore_oidc_instance', instance.id)
      } catch {
        // sessionStorage unavailable
      }

      // Redirect to IdP
      window.location.href = data.authorization_url
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Login failed')
      setLoading(false)
    }
  }

  return (
    <div className="flex-1 flex flex-col items-center justify-center p-6">
      <div className="w-full max-w-sm space-y-6">
        <div className="text-center space-y-2">
          <h1 className="text-2xl font-semibold tracking-tight">
            Sign in to oore.build
          </h1>
          <p className="text-muted-foreground text-sm">
            Authenticate with your identity provider to continue.
          </p>
        </div>

        <Card>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              Connected to{' '}
              <code className="bg-muted px-1.5 py-0.5 text-xs font-mono">
                {instance.label}
              </code>
            </p>

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
