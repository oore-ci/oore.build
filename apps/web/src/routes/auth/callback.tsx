import { createEffect, createSignal, Show } from 'solid-js'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import type { OidcCallbackResponse } from '@/lib/types'
import { setupOidcVerify } from '@/lib/api'
import { precheckOidcCallback } from '@/lib/oidc-callback'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import { PageMeta } from '@/lib/seo'

export const Route = createFileRoute('/auth/callback')({
  component: AuthCallbackPage,
})

function cleanupOidcSessionStorage() {
  try {
    sessionStorage.removeItem('oore_oidc_state')
    sessionStorage.removeItem('oore_oidc_instance')
    sessionStorage.removeItem('oore_oidc_flow')
    sessionStorage.removeItem('oore_setup_session_token')
  } catch {
    // ignore
  }
}

async function fetchOidcCallbackWithTimeout(
  url: string,
  body: { code: string; state: string },
  timeoutMs: number,
): Promise<OidcCallbackResponse> {
  const controller = new AbortController()
  const timeoutId = setTimeout(() => controller.abort(), timeoutMs)

  try {
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: controller.signal,
    })

    if (!response.ok) {
      const payload = (await response.json().catch(() => ({}))) as {
        error?: string
      }
      throw new Error(payload.error ?? `Authentication failed (${response.status})`)
    }

    return (await response.json()) as OidcCallbackResponse
  } catch (error) {
    if (controller.signal.aborted) {
      throw new Error(`Authentication callback timed out after ${timeoutMs}ms`)
    }
    throw error
  } finally {
    clearTimeout(timeoutId)
  }
}

function AuthCallbackPage() {
  const navigate = useNavigate()
  const setAuth = useAuthStore((state) => state.setAuth)

  const [error, setError] = createSignal<string | null>(null)
  const [errorHint, setErrorHint] = createSignal<string | null>(null)
  const [errorTarget, setErrorTarget] = createSignal<'/login' | '/setup/owner'>(
    '/login',
  )
  const [exchangeStarted, setExchangeStarted] = createSignal(false)

  const failAuth = (message: string, flow: string | null, hint: string) => {
    cleanupOidcSessionStorage()
    setError(message)
    setErrorHint(hint)
    setErrorTarget(flow === 'setup_owner' ? '/setup/owner' : '/login')
  }

  const handleSetupOwnerFlow = (
    code: string | undefined,
    state: string | undefined,
    setupSessionToken: string | null,
  ) => {
    if (!code || !state) {
      failAuth(
        'Missing callback parameters. Restart setup authentication.',
        'setup_owner',
        'missing_setup_callback_params',
      )
      return
    }

    if (!setupSessionToken) {
      failAuth(
        'Missing setup session token. Restart setup from the token step.',
        'setup_owner',
        'missing_setup_session_token',
      )
      return
    }

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

    void setupOidcVerify(instance.url, setupSessionToken, code, state)
      .then(() => {
        cleanupOidcSessionStorage()
        void navigate({ to: '/setup/complete' })
      })
      .catch((value: unknown) => {
        failAuth(
          value instanceof Error ? value.message : 'Setup owner verification failed',
          'setup_owner',
          'setup_owner_verify_failed',
        )
      })
  }

  const handleAuthFlow = (
    code: string | undefined,
    state: string | undefined,
    instanceId: string | null,
  ) => {
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

    useAuthStore.getState().setInstanceContext(instance.id)

    void fetchOidcCallbackWithTimeout(
      `${instance.url}/v1/auth/oidc/callback`,
      { code, state },
      8_000,
    )
      .then((data) => {
        if (!data.user.user_id || !data.user.role) {
          throw new Error('Incomplete user profile received from server')
        }

        setAuth()(
          data.session_token,
          data.expires_at,
          {
            email: data.user.email,
            oidc_subject: data.user.oidc_subject,
            user_id: data.user.user_id,
            role: data.user.role,
            avatar_url: data.user.avatar_url,
          },
        )

        cleanupOidcSessionStorage()
        void navigate({ to: '/' })
      })
      .catch((value: unknown) => {
        failAuth(
          value instanceof Error ? value.message : 'Authentication failed',
          'auth',
          'auth_callback_exchange_failed',
        )
      })
  }

  createEffect(() => {
    if (exchangeStarted()) return
    setExchangeStarted(true)

    let storedState: string | null = null
    let instanceId: string | null = null
    let flow: string | null = null
    let setupSessionToken: string | null = null

    try {
      storedState = sessionStorage.getItem('oore_oidc_state')
      instanceId = sessionStorage.getItem('oore_oidc_instance')
      flow = sessionStorage.getItem('oore_oidc_flow')
      setupSessionToken = sessionStorage.getItem('oore_setup_session_token')
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

    if (precheck.flow === 'setup_owner') {
      handleSetupOwnerFlow(precheck.code, precheck.state, setupSessionToken)
    } else {
      handleAuthFlow(precheck.code, precheck.state, instanceId)
    }
  })

  return (
    <div class="flex min-h-[60vh] items-center justify-center p-6">
      <PageMeta title="Signing In" />
      <div class="w-full max-w-md space-y-4">
        <Show
          when={error()}
          fallback={
            <div class="flex items-center justify-center gap-3">
              <Spinner class="size-5" />
              <p class="text-sm text-muted-foreground">Completing sign-in...</p>
            </div>
          }
        >
          <Alert variant="destructive">
            <AlertTitle>
              {errorTarget() === '/setup/owner'
                ? 'Setup Authentication Failed'
                : 'Authentication Failed'}
            </AlertTitle>
            <AlertDescription>{error()}</AlertDescription>
          </Alert>
          <Show when={errorHint()}>
            <p class="text-xs text-muted-foreground">
              Diagnostic hint: <code>{errorHint()}</code>
            </p>
          </Show>
          <Button variant="outline" onClick={() => void navigate({ to: errorTarget() })}>
            {errorTarget() === '/setup/owner' ? 'Back to Setup Owner' : 'Back to Login'}
          </Button>
        </Show>
      </div>
    </div>
  )
}
