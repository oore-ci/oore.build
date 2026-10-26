import { Link, createFileRoute } from '@tanstack/react-router'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { Collapsible, CollapsibleContent } from '@/components/ui/collapsible'
import {
  useCompleteSetup,
  useSetupStatus,
  useSetupSummary,
} from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { useAuthStore } from '@/stores/auth-store'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { PageMeta } from '@/lib/seo'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'
import { SetupStepError } from '@/components/setup-route-components'
import { Row } from '@/components/layout/row'
import { Text } from '@/components/layout/text'

export const Route = createFileRoute('/setup/complete')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
  component: CompleteStep,
  errorComponent: SetupStepError,
})

function CompleteStep() {
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const completeMutation = useCompleteSetup()
  const { data: status, isError: statusFailed } = useSetupStatus()
  const { data: summary } = useSetupSummary()

  const errorMessage = completeMutation.error
    ? getApiErrorMessage(completeMutation.error, {
        already_configured: 'Setup has already been completed.',
        session_expired:
          'Your setup session has expired. Please restart setup with a new bootstrap token.',
        invalid_session:
          'Your session is no longer valid. Please restart setup.',
      })
    : null

  const instanceId = completeMutation.data?.instance_id ?? null
  const isComplete = completeMutation.isSuccess
  const isLocalMode = status?.runtime_mode === 'local'
  const isOidcMode =
    status?.runtime_mode === 'remote' && status.remote_auth_mode === 'oidc'
  const accessLabel = isLocalMode
    ? 'This Mac only'
    : isOidcMode
      ? 'Team sign-in with an identity provider'
      : status?.remote_auth_mode === 'trusted_proxy'
        ? 'Trusted access proxy'
        : 'Unknown'

  function handleComplete() {
    if (!sessionToken) return
    completeMutation.mutate(sessionToken, {
      onSuccess: () => {
        if (isOidcMode) {
          useAuthStore.getState().clearAuth()
        }
        useSetupStore.getState().setSessionToken(null)
      },
    })
  }

  return (
    <div className="space-y-4">
      <PageMeta title="Setup Complete" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">Complete setup</h2>
        <Text variant="muted">
          Review your configuration and finalize the instance.
        </Text>
      </div>

      <div className="grid [&>[data-slot=collapsible]]:col-start-1 [&>[data-slot=collapsible]]:row-start-1">
        <Collapsible
          open={!isComplete}
          className={isComplete ? 'pointer-events-none' : undefined}
        >
          <CollapsibleContent className="data-ending-style:-translate-x-2 data-ending-style:translate-y-0 data-starting-style:-translate-x-2 data-starting-style:translate-y-0">
            <div className="space-y-4">
              {/* Configuration review */}
              {status || summary ? (
                <Card size="sm">
                  <CardHeader>
                    <CardTitle>Configuration summary</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2">
                      <dt className="text-muted-foreground">Access</dt>
                      <dd className="text-right">{accessLabel}</dd>
                      {summary?.owner_email ? (
                        <>
                          <dt className="text-muted-foreground">Owner</dt>
                          <dd className="truncate text-right">
                            {summary.owner_email}
                          </dd>
                        </>
                      ) : null}
                      {isOidcMode && summary?.issuer_url ? (
                        <>
                          <dt className="text-muted-foreground">
                            Identity provider
                          </dt>
                          <dd
                            className="truncate text-right font-mono text-xs leading-5"
                            title={summary.issuer_url}
                          >
                            {summary.issuer_url}
                          </dd>
                        </>
                      ) : null}
                      <dt className="text-muted-foreground">Instance ID</dt>
                      <dd
                        className="truncate text-right font-mono text-xs leading-5"
                        title={summary?.instance_id ?? status?.instance_id}
                      >
                        {summary?.instance_id ?? status?.instance_id}
                      </dd>
                    </dl>
                  </CardContent>
                </Card>
              ) : null}

              <Alert>
                <AlertTitle>Finalize setup</AlertTitle>
                <AlertDescription>
                  This locks the setup wizard so it cannot run again. You can
                  still change sign-in, preferences and users from Settings
                  afterwards.
                  {!isLocalMode
                    ? ' Verify your OIDC or proxy configuration is correct before proceeding.'
                    : ''}
                </AlertDescription>
              </Alert>

              {errorMessage ? (
                <Alert variant="destructive">
                  <AlertTitle>Completion failed</AlertTitle>
                  <AlertDescription>{errorMessage}</AlertDescription>
                </Alert>
              ) : null}

              {statusFailed && !status ? (
                <Alert variant="destructive">
                  <AlertTitle>Could not confirm the access mode</AlertTitle>
                  <AlertDescription>
                    Reload this page before you complete setup. Oore needs the
                    access mode to choose the correct sign-in handoff.
                  </AlertDescription>
                </Alert>
              ) : null}

              <Button
                onClick={handleComplete}
                disabled={!status || completeMutation.isPending}
                className="w-full"
              >
                {completeMutation.isPending
                  ? 'Completing...'
                  : 'Complete setup'}
              </Button>
            </div>
          </CollapsibleContent>
        </Collapsible>

        <Collapsible
          open={isComplete}
          className={!isComplete ? 'pointer-events-none' : undefined}
        >
          <CollapsibleContent className="data-ending-style:translate-x-2 data-ending-style:translate-y-0 data-starting-style:translate-x-2 data-starting-style:translate-y-0">
            <div className="space-y-4">
              <Alert>
                <AlertTitle>Access setup complete</AlertTitle>
                <AlertDescription>
                  The owner and sign-in settings are saved. If{' '}
                  <code>oore setup</code> opened this page, return to that
                  terminal. Wait for <strong>Complete setup is ready</strong>{' '}
                  before using Oore. The terminal still starts and checks the
                  device services.
                </AlertDescription>
              </Alert>

              {!isLocalMode ? (
                <Alert>
                  <AlertTitle>Confirm secure network access</AlertTitle>
                  <AlertDescription>
                    Oore configured sign-in, but it did not publish this Mac.
                    Connect your HTTPS proxy or tunnel before other people use
                    this instance. For OIDC, add that HTTPS address to the
                    provider redirect settings.
                  </AlertDescription>
                </Alert>
              ) : null}

              {isOidcMode ? (
                <Alert>
                  <AlertTitle>Sign in once more</AlertTitle>
                  <AlertDescription>
                    Setup verified the owner account. Sign in again to start a
                    normal Oore session. Your identity provider can reuse its
                    current sign-in.
                  </AlertDescription>
                </Alert>
              ) : null}

              <Row gap={2} className="text-sm">
                <span className="text-muted-foreground">Instance ID:</span>
                <Badge variant="outline" className="font-mono text-xs">
                  {instanceId}
                </Badge>
              </Row>

              <div className="space-y-4">
                <Separator />
                {isOidcMode ? (
                  <Button
                    render={<Link to="/login" />}
                    nativeButton={false}
                    className="w-full"
                  >
                    Sign in to Oore
                  </Button>
                ) : (
                  <Button
                    render={<Link to="/" />}
                    nativeButton={false}
                    className="w-full"
                  >
                    Build your first app
                  </Button>
                )}
              </div>
            </div>
          </CollapsibleContent>
        </Collapsible>
      </div>
    </div>
  )
}
