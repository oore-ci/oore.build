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
        <p className="text-sm text-muted-foreground">
          Review your configuration and finalize the instance.
        </p>
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
                  <CardContent className="space-y-2">
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">State</span>
                      <Badge variant="secondary" className="text-xs">
                        {status?.state ?? summary?.state}
                      </Badge>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Instance</span>
                      <code className="font-mono text-xs">
                        {summary?.instance_id ?? status?.instance_id}
                      </code>
                    </div>
                    {summary?.issuer_url ? (
                      <div className="flex justify-between gap-4">
                        <span className="shrink-0 text-muted-foreground">
                          OIDC Issuer
                        </span>
                        <code className="truncate font-mono text-xs">
                          {summary.issuer_url}
                        </code>
                      </div>
                    ) : null}
                    {summary?.owner_email ? (
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Owner</span>
                        <span className="text-xs">{summary.owner_email}</span>
                      </div>
                    ) : null}
                  </CardContent>
                </Card>
              ) : null}

              <Alert>
                <AlertTitle>Finalize setup</AlertTitle>
                <AlertDescription>
                  This will lock down the initial setup wizard so it cannot be
                  re-run. You can still change settings (authentication,
                  preferences, users) from the admin panel after setup is
                  complete.
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

              <div className="flex items-center gap-2 text-sm">
                <span className="text-muted-foreground">Instance ID:</span>
                <Badge variant="outline" className="font-mono text-xs">
                  {instanceId}
                </Badge>
              </div>

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
