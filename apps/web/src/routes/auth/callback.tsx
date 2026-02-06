import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useRef, useState } from 'react'
import type { OidcCallbackResponse } from '@/lib/types'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'

export const Route = createFileRoute('/auth/callback')({
  component: AuthCallbackPage,
})

function cleanupOidcSessionStorage() {
  try {
    sessionStorage.removeItem('oore_oidc_state')
    sessionStorage.removeItem('oore_oidc_instance')
  } catch {
    // ignore
  }
}

function AuthCallbackPage() {
  const navigate = useNavigate()
  const setAuth = useAuthStore((s) => s.setAuth)
  const [error, setError] = useState<string | null>(null)
  const exchangeStartedRef = useRef(false)

  useEffect(() => {
    document.title = 'Authenticating... - oore.build'
  }, [])

  useEffect(() => {
    // Guard against React StrictMode double-execution.
    // The backend consumes the OIDC state on first use, so a second
    // request with the same state would fail with a CSRF error.
    if (exchangeStartedRef.current) return
    exchangeStartedRef.current = true

    const params = new URLSearchParams(window.location.search)
    const code = params.get('code')
    const state = params.get('state')

    if (!code || !state) {
      setError('Missing authorization code or state parameter.')
      return
    }

    // Retrieve stored OIDC state
    let storedState: string | null = null
    let instanceId: string | null = null
    try {
      storedState = sessionStorage.getItem('oore_oidc_state')
      instanceId = sessionStorage.getItem('oore_oidc_instance')
    } catch {
      // sessionStorage unavailable
    }

    if (storedState !== state) {
      cleanupOidcSessionStorage()
      setError('OIDC state mismatch. Please try logging in again.')
      return
    }

    // Resolve the instance URL
    const instances = useInstanceStore.getState().instances

    const instance = instanceId ? instances[instanceId] : undefined
    if (!instance) {
      cleanupOidcSessionStorage()
      setError('Could not find the instance you were logging into.')
      return
    }

    // Sync auth store context before storing the token
    useAuthStore.getState().setInstanceContext(instance.id)

    // Exchange code for token via POST (keeps auth code out of URL/logs)
    const callbackUrl = `${instance.url}/v1/auth/oidc/callback`

    fetch(callbackUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code, state }),
    })
      .then(async (res) => {
        if (!res.ok) {
          const body = (await res.json().catch(() => ({}))) as {
            error?: string
          }
          throw new Error(body.error ?? `Authentication failed (${res.status})`)
        }
        return res.json() as Promise<OidcCallbackResponse>
      })
      .then((data) => {
        if (!data.user.user_id || !data.user.role) {
          throw new Error('Incomplete user profile received from server')
        }

        // Store auth token
        setAuth(data.session_token, data.expires_at, {
          email: data.user.email,
          oidc_subject: data.user.oidc_subject,
          user_id: data.user.user_id,
          role: data.user.role,
          avatar_url: data.user.avatar_url,
        })

        cleanupOidcSessionStorage()
        void navigate({ to: '/' })
      })
      .catch((e: unknown) => {
        cleanupOidcSessionStorage()
        setError(e instanceof Error ? e.message : 'Authentication failed')
      })
  }, [navigate, setAuth])

  if (error) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-6">
        <div className="w-full max-w-sm space-y-4">
          <Alert variant="destructive">
            <AlertTitle>Authentication Failed</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
          <div className="text-center">
            <Button
              variant="link"
              onClick={() => void navigate({ to: '/login' })}
            >
              Try again
            </Button>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="flex items-center gap-3">
        <Spinner className="size-5" />
        <p className="text-muted-foreground text-sm">Completing sign-in...</p>
      </div>
    </div>
  )
}
