import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useCallback, useEffect } from 'react'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { useSetupOidcStart, useSetupStatus } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { ApiClientError } from '@/lib/api'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'

export const Route = createFileRoute('/setup/owner')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
  component: OwnerStep,
  errorComponent: OwnerStepError,
})

function OwnerStepError({ error }: { error: Error }) {
  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <AlertTitle>Something went wrong</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
    </div>
  )
}

function getErrorMessage(error: Error | null): string | null {
  if (!error) return null
  if (error instanceof ApiClientError) {
    switch (error.code) {
      case 'invalid_state':
        return 'Owner has already been configured for this instance.'
      case 'session_expired':
        return 'Your setup session has expired. Please restart setup with a new bootstrap token.'
      case 'invalid_session':
        return 'Your session is no longer valid. Please restart setup.'
      case 'auth_expired':
        return 'The OIDC authorization request has expired. Please try again.'
      default:
        return error.message
    }
  }
  return error.message
}

function OwnerStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setCurrentStep = useSetupStore((s) => s.setCurrentStep)
  const startMutation = useSetupOidcStart()
  const { data: status } = useSetupStatus()

  const errorMessage = getErrorMessage(startMutation.error)

  useEffect(() => {
    setCurrentStep(2)
  }, [setCurrentStep])

  // Navigate based on backend state. The setup-status query polls every 3s.
  // Once the backend transitions to owner_created (whether from the unified
  // callback or any other path), we move forward.
  useEffect(() => {
    if (status?.state === 'owner_created') {
      setCurrentStep(3)
      void navigate({ to: '/setup/complete' })
    }
  }, [status?.state, setCurrentStep, navigate])

  const handleStartOidc = useCallback(() => {
    if (!sessionToken) return

    // Use the unified callback URI
    const redirectUri = `${window.location.origin}/auth/callback`
    startMutation.mutate(
      { sessionToken, redirectUri },
      {
        onSuccess: (data) => {
          // Store context so the unified callback knows this is a setup flow
          try {
            sessionStorage.setItem('oore_oidc_state', data.state)
            sessionStorage.setItem('oore_oidc_flow', 'setup_owner')
            sessionStorage.setItem('oore_setup_session_token', sessionToken)
          } catch {
            // ignore
          }
          // Redirect to the OIDC provider
          window.location.href = data.authorization_url
        },
      },
    )
  }, [sessionToken])

  const handleRestartFromToken = useCallback(() => {
    useSetupStore.getState().reset()
    void navigate({ to: '/setup' })
  }, [navigate])

  return (
    <div className="space-y-4">
      <PageMeta title="Setup Owner" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">Owner Account</h2>
        <p className="text-sm text-muted-foreground">
          Authenticate with your OIDC provider to verify your identity. Your
          email and OIDC subject will be extracted from the identity provider's
          ID token.
        </p>
      </div>

      <Alert>
        <AlertTitle>Troubleshooting tip</AlertTitle>
        <AlertDescription>
          If your provider shows errors like <code>invalid_client</code> or
          callback mismatch, go back to OIDC settings and update client ID,
          secret, and allowed redirect URI. If the provider does not redirect
          back, return here manually and retry after fixing OIDC settings.
        </AlertDescription>
      </Alert>

      {errorMessage ? (
        <Alert variant="destructive">
          <AlertTitle>Failed to start authentication</AlertTitle>
          <AlertDescription>{errorMessage}</AlertDescription>
        </Alert>
      ) : null}

      <Button
        onClick={handleStartOidc}
        disabled={startMutation.isPending}
        className="w-full"
      >
        {startMutation.isPending
          ? 'Redirecting...'
          : 'Authenticate with OIDC Provider'}
      </Button>

      <div className="grid gap-2 sm:grid-cols-2">
        <Button variant="outline" onClick={() => void navigate({ to: '/setup/oidc' })}>
          Back to OIDC Settings
        </Button>
        <Button variant="outline" onClick={handleRestartFromToken}>
          Restart from Token Step
        </Button>
      </div>
    </div>
  )
}
