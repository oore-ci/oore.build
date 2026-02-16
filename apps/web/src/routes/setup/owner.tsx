import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useCallback, useEffect } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  useSetupLocalOwnerCreate,
  useSetupOidcStart,
  useSetupStatus,
  useSetupTrustedProxyClaimOwner,
} from '@/hooks/use-setup'
import { ApiClientError, getApiErrorMessage } from '@/lib/api'
import { useSetupStore } from '@/stores/setup-store'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'

const localOwnerSchema = z.object({
  email: z.string().email('Enter a valid email address'),
})

type LocalOwnerForm = z.infer<typeof localOwnerSchema>

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

function getOidcErrorMessage(error: Error | null): string | null {
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
      case 'invalid_redirect_uri':
        if (
          error.message.includes('public redirect_uri must use https scheme') ||
          error.message.includes(
            'non-localhost redirect_uri must use https scheme',
          )
        ) {
          return 'This frontend URL cannot be used for OIDC callback over HTTP. Use an HTTPS frontend URL or open setup from localhost/.local.'
        }
        if (error.message.includes('origin is not in the allowed origins list')) {
          return 'This frontend origin is not allowed by the backend. Update allowed frontend origins in Preferences and try again.'
        }
        return 'OIDC callback URL is invalid. Ensure the callback path is /auth/callback.'
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
  const startOidcMutation = useSetupOidcStart()
  const localOwnerMutation = useSetupLocalOwnerCreate()
  const trustedProxyClaimMutation = useSetupTrustedProxyClaimOwner()
  const { data: status } = useSetupStatus()
  const isLocalMode = status?.runtime_mode === 'local'
  const isTrustedProxyMode =
    status?.runtime_mode === 'remote' &&
    status?.remote_auth_mode === 'trusted_proxy'

  const localOwnerForm = useForm<LocalOwnerForm>({
    resolver: zodResolver(localOwnerSchema),
    defaultValues: {
      email: 'owner@local',
    },
  })

  const oidcErrorMessage = getOidcErrorMessage(startOidcMutation.error)
  const localErrorMessage = localOwnerMutation.error
    ? getApiErrorMessage(localOwnerMutation.error, {
        invalid_input: 'Enter a valid email address.',
        session_expired:
          'Your setup session has expired. Restart setup with a new bootstrap token.',
        invalid_session:
          'Your session is no longer valid. Restart setup from the token step.',
        mode_restricted:
          'Local owner creation is only available in Local Only mode.',
      })
    : null
  const trustedProxyErrorMessage = trustedProxyClaimMutation.error
    ? getApiErrorMessage(trustedProxyClaimMutation.error, {
        trusted_proxy_peer_not_allowed:
          'This request did not come from a trusted proxy peer.',
        trusted_proxy_identity_missing:
          'Trusted proxy identity header is missing. Check Warpgate header forwarding.',
        trusted_proxy_identity_invalid:
          'Trusted proxy identity header must contain an email address.',
        mode_restricted:
          'Switch setup mode to Remote (Trusted Proxy) before claiming owner.',
      })
    : null

  useEffect(() => {
    setCurrentStep(isLocalMode ? 2 : 3)
  }, [isLocalMode, setCurrentStep])

  useEffect(() => {
    if (status?.state === 'owner_created') {
      setCurrentStep(isLocalMode ? 3 : 4)
      void navigate({ to: '/setup/complete' })
    }
  }, [status?.state, isLocalMode, setCurrentStep, navigate])

  const handleStartOidc = useCallback(() => {
    if (!sessionToken) return

    const redirectUri = `${window.location.origin}/auth/callback`
    startOidcMutation.mutate(
      { sessionToken, redirectUri },
      {
        onSuccess: (data) => {
          try {
            sessionStorage.setItem('oore_oidc_state', data.state)
            sessionStorage.setItem('oore_oidc_flow', 'setup_owner')
            sessionStorage.setItem('oore_setup_session_token', sessionToken)
          } catch {
            // ignore
          }
          window.location.href = data.authorization_url
        },
      },
    )
  }, [sessionToken, startOidcMutation])

  function handleCreateLocalOwner(data: LocalOwnerForm) {
    if (!sessionToken) return
    localOwnerMutation.mutate(
      {
        sessionToken,
        email: data.email.trim().toLowerCase(),
      },
      {
        onSuccess: () => {
          setCurrentStep(3)
          void navigate({ to: '/setup/complete' })
        },
      },
    )
  }

  const handleClaimTrustedProxyOwner = useCallback(() => {
    if (!sessionToken) return
    trustedProxyClaimMutation.mutate(
      { sessionToken },
      {
        onSuccess: () => {
          setCurrentStep(4)
          void navigate({ to: '/setup/complete' })
        },
      },
    )
  }, [sessionToken, trustedProxyClaimMutation, setCurrentStep, navigate])

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
          {isLocalMode
            ? 'Create a local owner account to finish setup without OIDC.'
            : isTrustedProxyMode
              ? 'Confirm owner identity from your trusted proxy (Warpgate).'
            : "Authenticate with your OIDC provider to verify your identity. Your email and OIDC subject will be extracted from the provider's ID token."}
        </p>
      </div>

      {isLocalMode ? (
        <Form {...localOwnerForm}>
          <form
            onSubmit={localOwnerForm.handleSubmit(handleCreateLocalOwner)}
            className="space-y-4"
          >
            <FormField
              control={localOwnerForm.control}
              name="email"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Owner email</FormLabel>
                  <FormControl>
                    <Input
                      {...field}
                      placeholder="owner@local"
                      autoComplete="email"
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            {localErrorMessage ? (
              <Alert variant="destructive">
                <AlertTitle>Failed to create owner</AlertTitle>
                <AlertDescription>{localErrorMessage}</AlertDescription>
              </Alert>
            ) : null}

            <Button
              type="submit"
              className="w-full"
              disabled={localOwnerMutation.isPending}
            >
              {localOwnerMutation.isPending
                ? 'Creating owner...'
                : 'Create Local Owner'}
            </Button>
          </form>
        </Form>
      ) : isTrustedProxyMode ? (
        <>
          {trustedProxyErrorMessage ? (
            <Alert variant="destructive">
              <AlertTitle>Failed to claim owner identity</AlertTitle>
              <AlertDescription>{trustedProxyErrorMessage}</AlertDescription>
            </Alert>
          ) : null}

          <Button
            onClick={handleClaimTrustedProxyOwner}
            disabled={trustedProxyClaimMutation.isPending}
            className="w-full"
          >
            {trustedProxyClaimMutation.isPending
              ? 'Claiming owner...'
              : 'Confirm Owner from Trusted Proxy'}
          </Button>

          <Button
            variant="outline"
            onClick={() => void navigate({ to: '/setup/trusted-proxy' })}
            className="w-full"
          >
            Back to Trusted Proxy Settings
          </Button>
        </>
      ) : (
        <>
          <Alert>
            <AlertTitle>Troubleshooting tip</AlertTitle>
            <AlertDescription>
              If your provider shows errors like <code>invalid_client</code> or
              callback mismatch, go back to OIDC settings and update client ID,
              secret, and allowed redirect URI. If the provider does not
              redirect back, return here manually and retry after fixing OIDC
              settings.
            </AlertDescription>
          </Alert>

          {oidcErrorMessage ? (
            <Alert variant="destructive">
              <AlertTitle>Failed to start authentication</AlertTitle>
              <AlertDescription>{oidcErrorMessage}</AlertDescription>
            </Alert>
          ) : null}

          <Button
            onClick={handleStartOidc}
            disabled={startOidcMutation.isPending}
            className="w-full"
          >
            {startOidcMutation.isPending
              ? 'Redirecting...'
              : 'Authenticate with OIDC Provider'}
          </Button>

          <Button
            variant="outline"
            onClick={() => void navigate({ to: '/setup/oidc' })}
            className="w-full"
          >
            Back to OIDC Settings
          </Button>
        </>
      )}

      <Button variant="outline" onClick={handleRestartFromToken} className="w-full">
        Restart from Token Step
      </Button>
    </div>
  )
}
