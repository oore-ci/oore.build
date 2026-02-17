import { Link, createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { Button, buttonVariants } from '@/components/ui/button'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import {
  useCompleteSetup,
  useSetupStatus,
  useSetupSummary,
} from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { getApiErrorMessage } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/setup/complete')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
  component: CompleteStep,
  errorComponent: CompleteStepError,
})

function CompleteStepError({ error }: { error: Error }) {
  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <AlertTitle>Something went wrong</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
    </div>
  )
}

function CompleteStep() {
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setCurrentStep = useSetupStore((s) => s.setCurrentStep)
  const completeMutation = useCompleteSetup()
  const { data: status } = useSetupStatus()
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

  useEffect(() => {
    setCurrentStep(isLocalMode ? 3 : 4)
  }, [isLocalMode, setCurrentStep])

  function handleComplete() {
    if (!sessionToken) return
    completeMutation.mutate(sessionToken)
  }

  // Clean up session token after completion, but keep step at 4
  // so the indicator shows all steps as completed
  useEffect(() => {
    if (isComplete) {
      setCurrentStep(isLocalMode ? 4 : 5)
      useSetupStore.getState().setSessionToken(null)
    }
  }, [isComplete, isLocalMode, setCurrentStep])

  return (
    <div className="space-y-4">
      <PageMeta title="Setup Complete" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">Complete Setup</h2>
        <p className="text-sm text-muted-foreground">
          Review your configuration and finalize the instance.
        </p>
      </div>

      {isComplete ? (
        <div className="space-y-4">
          <Alert>
            <AlertTitle>Setup complete</AlertTitle>
            <AlertDescription>
              Your Oore instance is ready. Setup endpoints have been
              permanently disabled.
            </AlertDescription>
          </Alert>

          <div className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground">Instance ID:</span>
            <Badge variant="outline" className="font-mono text-xs">
              {instanceId}
            </Badge>
          </div>

          <div className="border-t pt-4">
            <Link to="/" className={buttonVariants({ className: 'w-full' })}>
              Go to Dashboard
            </Link>
          </div>
        </div>
      ) : (
        <div className="space-y-4">
          {/* Configuration review */}
          {status || summary ? (
            <div className="border p-3 space-y-2 text-sm">
              <p className="font-medium text-xs uppercase tracking-wider text-muted-foreground">
                Configuration Summary
              </p>
              <div className="flex justify-between">
                <span className="text-muted-foreground">State</span>
                <Badge variant="secondary" className="text-xs">
                  {status?.state ?? summary?.state}
                </Badge>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Instance</span>
                <code className="text-xs font-mono">
                  {summary?.instance_id ?? status?.instance_id}
                </code>
              </div>
              {summary?.issuer_url ? (
                <div className="flex justify-between gap-4">
                  <span className="text-muted-foreground shrink-0">
                    OIDC Issuer
                  </span>
                  <code className="text-xs font-mono truncate">
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
            </div>
          ) : null}

          <Alert variant="destructive">
            <AlertTitle>Warning — Irreversible action</AlertTitle>
            <AlertDescription>
              Completing setup will permanently lock down all setup endpoints.
              This cannot be undone. Make sure your owner details
              {isLocalMode
                ? ''
                : ' and remote authentication configuration'}{' '}
              are correct before proceeding.
            </AlertDescription>
          </Alert>

          {errorMessage ? (
            <Alert variant="destructive">
              <AlertTitle>Completion failed</AlertTitle>
              <AlertDescription>{errorMessage}</AlertDescription>
            </Alert>
          ) : null}

          <Button
            onClick={handleComplete}
            disabled={completeMutation.isPending}
            className="w-full"
          >
            {completeMutation.isPending ? 'Completing...' : 'Complete Setup'}
          </Button>
        </div>
      )}
    </div>
  )
}
