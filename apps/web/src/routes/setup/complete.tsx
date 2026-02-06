import { createFileRoute, Link, redirect } from '@tanstack/react-router'
import { useEffect } from 'react'
import { buttonVariants } from '@/components/ui/button'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { useCompleteSetup, useSetupStatus } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'
import { ApiClientError } from '@/lib/api'

export const Route = createFileRoute('/setup/complete')({
  beforeLoad: () => {
    const sessionToken = useSetupStore.getState().sessionToken
    if (!sessionToken) {
      throw redirect({ to: '/setup' })
    }
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

  const errorMessage = completeMutation.error
    ? completeMutation.error instanceof ApiClientError
      ? completeMutation.error.code === 'already_configured'
        ? 'Setup has already been completed.'
        : completeMutation.error.code === 'session_expired'
          ? 'Your setup session has expired. Please restart setup with a new bootstrap token.'
          : completeMutation.error.code === 'invalid_session'
            ? 'Your session is no longer valid. Please restart setup.'
            : completeMutation.error.message
      : completeMutation.error.message
    : null

  const instanceId = completeMutation.data?.instance_id ?? null
  const isComplete = completeMutation.isSuccess

  useEffect(() => {
    setCurrentStep(3)
  }, [setCurrentStep])

  function handleComplete() {
    if (!sessionToken) return
    completeMutation.mutate(sessionToken)
  }

  // Clean up session token after completion, but keep step at 4
  // so the indicator shows all steps as completed
  useEffect(() => {
    if (isComplete) {
      setCurrentStep(4)
      useSetupStore.getState().setSessionToken(null)
    }
  }, [isComplete, setCurrentStep])

  return (
    <div className="space-y-4">
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
              Your oore.build instance is ready. Setup endpoints have been
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
          {status ? (
            <div className="border p-3 space-y-2 text-sm">
              <p className="font-medium text-xs uppercase tracking-wider text-muted-foreground">
                Configuration Summary
              </p>
              <div className="flex justify-between">
                <span className="text-muted-foreground">State</span>
                <Badge variant="secondary" className="text-xs">{status.state}</Badge>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Instance</span>
                <code className="text-xs">{status.instance_id.slice(0, 8)}...</code>
              </div>
            </div>
          ) : null}

          <Alert variant="destructive">
            <AlertTitle>Warning — Irreversible action</AlertTitle>
            <AlertDescription>
              Completing setup will permanently lock down all setup endpoints.
              This cannot be undone. Make sure your OIDC configuration and owner
              email are correct before proceeding.
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
            {completeMutation.isPending
              ? 'Completing...'
              : 'Complete Setup'}
          </Button>
        </div>
      )}
    </div>
  )
}
