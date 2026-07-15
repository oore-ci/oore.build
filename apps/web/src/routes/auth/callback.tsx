import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useRef, useState } from 'react'
import type { OidcCallbackResponse } from '@/lib/types'
import { useMountEffect } from '@/hooks/use-mount-effect'
import { setupOidcVerify } from '@/lib/api'
import { precheckOidcCallback } from '@/lib/oidc-callback'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import { PageMeta } from '@/lib/seo'
import { resolveRequiredInstanceApiBaseUrl } from '@/lib/instance-url'

export const Route = createFileRoute('/auth/callback')({
  component: AuthCallbackPage,
})

function cleanupOidcSessionStorage() {
  try {
    sessionStorage.removeItem('oore_oidc_state')
    sessionStorage.removeItem('oore_oidc_instance')
    sessionStorage.removeItem('oore_oidc_flow')
  } catch {
    // ignore
  }
}

function AuthCallbackPage() {
  const navigate = useNavigate()
  const setAuth = useAuthStore((s) => s.setAuth)
  const [error, setError] = useState<string | null>(null)
  const [errorHint, setErrorHint] = useState<string | null>(null)
  const [errorTarget, setErrorTarget] = useState<'/login' | '/setup/owner'>(
    '/login',
  )
  const exchangeStartedRef = useRef(false)

  useMountEffect(() => {
    // Guard against React StrictMode double-execution.
    if (exchangeStartedRef.current) return
    exchangeStartedRef.current = true

    // Retrieve stored OIDC context
    let storedState: string | null = null
    let instanceId: string | null = null
    let flow: string | null = null
    try {
      storedState = sessionStorage.getItem('oore_oidc_state')
      instanceId = sessionStorage.getItem('oore_oidc_instance')
      flow = sessionStorage.getItem('oore_oidc_flow')
    } catch {
      failAuth(
        'Unable to access browser session storage. Restart sign-in from the app.',
        null,
        'session_storage_unavailable',
      )
      return
    }

    const params = new URLSearchParams(window.location.search)
    const precheck = precheckOidcCallback(params, storedState, flow)
    setErrorTarget(precheck.target)

    if (!precheck.ok) {
      failAuth(
        precheck.message ?? 'Authentication callback validation failed.',
        precheck.flow,
        precheck.hint ?? 'callback_validation_failed',
      )
      return
    }

    // Route based on flow type
    if (precheck.flow === 'setup_owner') {
      handleSetupOwnerFlow(precheck.code, precheck.state)
    } else {
      handleAuthFlow(precheck.code, precheck.state, instanceId)
    }
  })

  function failAuth(message: string, flow: string | null, hint: string) {
    cleanupOidcSessionStorage()
    setError(() => message)
    setErrorHint(() => hint)
    setErrorTarget(flow === 'setup_owner' ? '/setup/owner' : '/login')
  }

  function handleSetupOwnerFlow(
    code: string | undefined,
    state: string | undefined,
  ) {
    if (!code || !state) {
      failAuth(
        'Missing callback parameters. Restart setup authentication.',
        'setup_owner',
        'missing_setup_callback_params',
      )
      return
    }

    // Get the active instance URL for the API call
    const activeId = useInstanceStore.getState().activeInstanceId
    const instances = useInstanceStore.getState().instances
    const instance = activeId ? instances[activeId] : undefined

    if (!instance) {
      failAuth(
        'Could not find the active instance. Restart setup from the token step.',
        'setup_owner',
        'missing_active_instance',
      )
      return
    }

    // POST to verify-oidc with the setup session token
    setupOidcVerify(resolveRequiredInstanceApiBaseUrl(instance), code, state)
      .then(() => {
        cleanupOidcSessionStorage()
        void navigate({ to: '/setup/complete' })
      })
      .catch((e: unknown) => {
        failAuth(
          e instanceof Error ? e.message : 'Setup owner verification failed',
          'setup_owner',
          'setup_owner_verify_failed',
        )
      })
  }

  function handleAuthFlow(
    code: string | undefined,
    state: string | undefined,
    instanceId: string | null,
  ) {
    if (!code || !state) {
      failAuth(
        'Missing callback parameters. Restart sign-in.',
        'auth',
        'missing_auth_callback_params',
      )
      return
    }

    const instances = useInstanceStore.getState().instances
    const instance = instanceId ? instances[instanceId] : undefined

    if (!instance) {
      failAuth(
        'Could not find the instance you were logging into.',
        'auth',
        'missing_auth_instance',
      )
      return
    }

    // Sync auth store context
    useAuthStore.getState().setInstanceContext(instance.id)

    // Exchange code for token via POST
    const callbackUrl = `${resolveRequiredInstanceApiBaseUrl(instance)}/v1/auth/oidc/callback`

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
        failAuth(
          e instanceof Error ? e.message : 'Authentication failed',
          'auth',
          'auth_callback_exchange_failed',
        )
      })
  }

  if (error) {
    const actionLabel =
      errorTarget === '/setup/owner' ? 'Back to setup owner' : 'Back to login'
    const title =
      errorTarget === '/setup/owner'
        ? 'Setup authentication failed'
        : 'Authentication failed'
    return (
      <div className="focused-flow flex min-h-0 flex-1 flex-col items-center p-4 sm:p-6">
        <PageMeta title="Signing In" />
        <div className="w-full max-w-md space-y-4">
          <Alert variant="destructive">
            <AlertTitle>{title}</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
          {errorHint ? (
            <p className="text-xs text-muted-foreground">
              Diagnostic hint: <code>{errorHint}</code>
            </p>
          ) : null}
          <div className="text-center">
            <Button
              variant="link"
              onClick={() => void navigate({ to: errorTarget })}
            >
              {actionLabel}
            </Button>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="focused-flow flex min-h-0 flex-1 items-center p-4 sm:p-6">
      <PageMeta title="Signing In" />
      <div className="flex items-center gap-3">
        <Spinner className="size-5" />
        <p className="text-muted-foreground text-sm">Completing sign-in...</p>
      </div>
    </div>
  )
}
