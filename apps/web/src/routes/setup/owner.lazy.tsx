import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
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
import { Spinner } from '@/components/ui/spinner'
import {
  useSetupLocalOwnerCreate,
  useSetupStatus,
  useSetupTrustedProxyClaimOwner,
} from '@/hooks/use-setup'
import { useSetupOidcVerificationStart } from '@/hooks/use-authorization-start'
import { isOoreApiError } from '@oore/client/client'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { useSetupStore } from '@/stores/setup-store'
import { PageMeta } from '@/lib/seo'
import { useOwnerStepTransition } from '@/hooks/use-setup-route-transitions'
import { SetupStepError } from '@/components/setup-route-components'
import { Text } from '@/components/layout/text'

const localOwnerSchema = z.object({
  email: z
    .string()
    .trim()
    .min(3, 'Enter an account label such as owner@local')
    .max(254, 'The account label is too long')
    .regex(/^[^@\s]+@[^@\s]+$/, 'Enter an account label such as owner@local'),
})

type LocalOwnerForm = z.infer<typeof localOwnerSchema>

export const Route = createLazyFileRoute('/setup/owner')({
  component: OwnerStep,
  errorComponent: SetupStepError,
})

function getOidcErrorMessage(error: Error | null): string | null {
  if (!error) return null
  if (isOoreApiError(error)) {
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
        if (
          error.message.includes('origin is not in the allowed origins list')
        ) {
          return 'This frontend origin is not allowed by the backend. Update allowed frontend origins in Settings → General and try again.'
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
  const startOidcMutation = useSetupOidcVerificationStart()
  const localOwnerMutation = useSetupLocalOwnerCreate()
  const trustedProxyClaimMutation = useSetupTrustedProxyClaimOwner()
  const { data: status } = useSetupStatus()
  useOwnerStepTransition(status)

  const localOwnerForm = useForm<LocalOwnerForm>({
    resolver: zodResolver(localOwnerSchema),
    defaultValues: {
      email: 'owner@local',
    },
    mode: 'onBlur',
  })

  const oidcErrorMessage = getOidcErrorMessage(startOidcMutation.error)
  const localErrorMessage = localOwnerMutation.error
    ? getApiErrorMessage(localOwnerMutation.error, {
        invalid_input: 'Enter an account label such as owner@local.',
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
          'Trusted proxy identity header is missing. Check proxy header forwarding.',
        trusted_proxy_identity_invalid:
          'Trusted proxy identity header must contain an email address.',
        trusted_proxy_owner_email_mismatch:
          'The proxy-authenticated email does not match the configured initial owner email.',
        mode_restricted:
          'Switch setup mode to Remote (Trusted Proxy) before claiming owner.',
      })
    : null

  if (!status) {
    return (
      <div className="flex justify-center py-8">
        <Spinner className="size-5" />
      </div>
    )
  }

  const isLocalMode = status.runtime_mode === 'local'
  const isTrustedProxyMode =
    status.runtime_mode === 'remote' &&
    status.remote_auth_mode === 'trusted_proxy'

  function handleStartOidc() {
    if (!sessionToken) return

    const redirectUri = `${window.location.origin}/auth/callback`
    startOidcMutation.mutate(
      { sessionToken, redirectUri },
      {
        onSuccess: (data) => {
          try {
            sessionStorage.setItem('oore_oidc_state', data.state)
            sessionStorage.setItem('oore_oidc_flow', 'setup_owner')
          } catch {
            // ignore
          }
          window.location.href = data.authorization_url
        },
      },
    )
  }

  function handleCreateLocalOwner(data: LocalOwnerForm) {
    if (!sessionToken) return
    localOwnerMutation.mutate(
      {
        sessionToken,
        email: data.email.trim().toLowerCase(),
      },
      {
        onSuccess: () => {
          void navigate({
            to: '/setup/complete',
            viewTransition: { types: ['setup-forward'] },
          })
        },
      },
    )
  }

  function handleClaimTrustedProxyOwner() {
    if (!sessionToken) return
    trustedProxyClaimMutation.mutate(
      { sessionToken },
      {
        onSuccess: () => {
          void navigate({
            to: '/setup/complete',
            viewTransition: { types: ['setup-forward'] },
          })
        },
      },
    )
  }

  function handleRestartFromToken() {
    useSetupStore.getState().reset()
    void navigate({
      to: '/setup',
      viewTransition: { types: ['setup-back'] },
    })
  }

  return (
    <div className="space-y-4">
      <PageMeta title="Setup Owner" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">Owner account</h2>
        <Text variant="muted">
          {isLocalMode
            ? 'This email is only an account label on this Mac. Oore does not send email or ask for a password.'
            : isTrustedProxyMode
              ? 'Confirm owner identity from your trusted proxy.'
              : "Authenticate with your OIDC provider to verify your identity. Your email and OIDC subject will be extracted from the provider's ID token."}
        </Text>
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
                  <FormLabel>Local owner email</FormLabel>
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
                : 'Create local owner'}
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
              : 'Confirm owner from trusted proxy'}
          </Button>

          <Button
            variant="outline"
            onClick={() =>
              void navigate({
                to: '/setup/trusted-proxy',
                viewTransition: { types: ['setup-back'] },
              })
            }
            className="w-full"
          >
            Back to trusted proxy settings
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
              : 'Authenticate with OIDC provider'}
          </Button>

          <Button
            variant="outline"
            onClick={() =>
              void navigate({
                to: '/setup/oidc',
                viewTransition: { types: ['setup-back'] },
              })
            }
            className="w-full"
          >
            Back to OIDC settings
          </Button>
        </>
      )}

      <Button
        variant="ghost"
        size="sm"
        onClick={handleRestartFromToken}
        className="mx-auto flex text-muted-foreground"
      >
        Start over with a new token
      </Button>
    </div>
  )
}
