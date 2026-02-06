import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useCallback, useEffect, useRef } from 'react'
import { z } from 'zod'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import {
  useSetupOidcStart,
  useSetupOidcVerify,
  useSetupStatus,
} from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { ApiClientError } from '@/lib/api'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'

const ownerSearchSchema = z.object({
  code: z.string().optional(),
  state: z.string().optional(),
})

export const Route = createFileRoute('/setup/owner')({
  validateSearch: ownerSearchSchema,
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
  const { code, state: oidcState } = Route.useSearch()
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setCurrentStep = useSetupStore((s) => s.setCurrentStep)
  const startMutation = useSetupOidcStart()
  const verifyMutation = useSetupOidcVerify()
  const { data: status } = useSetupStatus()

  const errorMessage =
    getErrorMessage(startMutation.error) ??
    getErrorMessage(verifyMutation.error)

  useEffect(() => {
    document.title = 'Owner Account — oore.build'
  }, [])

  useEffect(() => {
    setCurrentStep(2)
  }, [setCurrentStep])

  // Guard against double-firing the verify mutation.
  const verifyAttempted = useRef(false)

  // Fire the verify request when we have code+state from the OIDC redirect.
  // The mutation response may or may not arrive (the full-page redirect to
  // the IdP and back can cause the fetch to behave unreliably). Navigation
  // is handled separately via setup-status polling below.
  useEffect(() => {
    if (code && oidcState && sessionToken && !verifyAttempted.current) {
      verifyAttempted.current = true
      verifyMutation.mutate(
        { sessionToken, code, state: oidcState },
        {
          onError: () => {
            verifyAttempted.current = false
          },
        },
      )
    }
  }, [code, oidcState, sessionToken])

  // Navigate based on backend state, not mutation response. The setup-status
  // query polls every 3 seconds. Once the backend transitions to owner_created
  // (whether we got the mutation response or not), we move forward.
  useEffect(() => {
    if (status?.state === 'owner_created') {
      setCurrentStep(3)
      void navigate({ to: '/setup/complete' })
    }
  }, [status?.state, setCurrentStep, navigate])

  const handleStartOidc = useCallback(() => {
    if (!sessionToken) return

    // The redirect URI is the current page so the OIDC provider redirects back here
    const redirectUri = `${window.location.origin}/setup/owner`
    startMutation.mutate(
      { sessionToken, redirectUri },
      {
        onSuccess: (data) => {
          // Store the OIDC state in sessionStorage so we can restore if needed
          try {
            sessionStorage.setItem('oore_oidc_state', data.state)
          } catch {
            // ignore
          }
          // Redirect to the OIDC provider
          window.location.href = data.authorization_url
        },
      },
    )
  }, [sessionToken])

  // Show verifying state while processing the callback
  if (code && oidcState) {
    return (
      <div className="space-y-4">
        <div className="space-y-1">
          <h2 className="text-lg font-medium">Owner Account</h2>
          <p className="text-sm text-muted-foreground">
            Verifying your identity with the OIDC provider...
          </p>
        </div>

        {errorMessage ? (
          <Alert variant="destructive">
            <AlertTitle>Authentication failed</AlertTitle>
            <AlertDescription>{errorMessage}</AlertDescription>
          </Alert>
        ) : null}

        {verifyMutation.isError ? (
          <Button onClick={handleStartOidc} className="w-full">
            Try Again
          </Button>
        ) : null}
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="space-y-1">
        <h2 className="text-lg font-medium">Owner Account</h2>
        <p className="text-sm text-muted-foreground">
          Authenticate with your OIDC provider to verify your identity. Your
          email and OIDC subject will be extracted from the identity provider's
          ID token.
        </p>
      </div>

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
    </div>
  )
}
