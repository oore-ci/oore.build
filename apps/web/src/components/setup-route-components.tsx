import { useNavigate, useRouter } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Tick02Icon } from '@hugeicons/core-free-icons'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { getConnectivityIssue, isHostedUiOrigin } from '@/lib/connectivity'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useInstanceStore } from '@/stores/instance-store'

export function SetupStepIndicator({
  currentStep,
  steps,
}: {
  currentStep: number
  steps: Array<string>
}) {
  const activeLabel =
    steps[Math.min(Math.max(currentStep, 0), steps.length - 1)] ?? ''

  return (
    <nav aria-label="Setup progress">
      <div className="flex justify-center sm:hidden">
        <Badge variant="secondary" aria-current="step">
          Step {Math.min(currentStep + 1, steps.length)} of {steps.length}
          <span className="mx-1 text-muted-foreground">·</span>
          {activeLabel}
        </Badge>
      </div>
      <div className="hidden items-center justify-center gap-1 sm:flex">
        {steps.map((label, index) => {
          const isActive = index === currentStep
          const isCompleted = index < currentStep

          return (
            <div key={label} className="flex items-center gap-1">
              {index > 0 ? (
                <div
                  className={`h-px w-8 ${isCompleted ? 'bg-primary' : 'bg-border'}`}
                />
              ) : null}
              <Badge
                variant={isActive ? 'secondary' : 'outline'}
                className="text-xs"
                aria-current={isActive ? 'step' : undefined}
              >
                {isCompleted ? (
                  <>
                    <HugeiconsIcon
                      icon={Tick02Icon}
                      size={12}
                      className="mr-0.5"
                    />
                    {label}
                  </>
                ) : (
                  label
                )}
              </Badge>
            </div>
          )
        })}
      </div>
    </nav>
  )
}

export function SetupStepError({ error }: { error: Error }) {
  const navigate = useNavigate()
  const router = useRouter()

  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <AlertTitle>Something went wrong</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
      <div className="flex flex-col gap-2 sm:flex-row">
        <Button onClick={() => void router.invalidate()}>Try again</Button>
        <Button
          variant="outline"
          onClick={() => void navigate({ to: '/setup' })}
        >
          Back to setup
        </Button>
      </div>
    </div>
  )
}

export function SetupRouteError({ error }: { error: Error }) {
  const navigate = useNavigate()
  const router = useRouter()
  const activeInstanceId = useInstanceStore((state) => state.activeInstanceId)
  const instances = useInstanceStore((state) => state.instances)
  const instance = activeInstanceId ? instances[activeInstanceId] : null
  const backendUrl = resolveInstanceApiBaseUrl(instance) ?? ''
  const frontendOrigin = window.location.origin
  const issue = backendUrl
    ? getConnectivityIssue(backendUrl, error, frontendOrigin)
    : null
  const hostedUi = isHostedUiOrigin(frontendOrigin)
  const goBack = () => {
    if (window.history.length > 1) {
      window.history.back()
      return
    }
    void navigate({ to: '/login' })
  }

  if (!issue) {
    return (
      <div className="focused-flow flex min-h-0 flex-1 items-center p-4 sm:p-6">
        <div className="w-full max-w-xl space-y-4">
          <Alert variant="destructive">
            <AlertTitle>Something went wrong</AlertTitle>
            <AlertDescription>{error.message}</AlertDescription>
          </Alert>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Button onClick={() => void router.invalidate()}>Retry</Button>
            <Button variant="outline" onClick={goBack}>
              Go back
            </Button>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="focused-flow flex min-h-0 flex-1 items-center p-4 sm:p-6">
      <div className="w-full max-w-xl space-y-4">
        <Alert variant="destructive">
          <AlertTitle>{issue.title}</AlertTitle>
          <AlertDescription>{issue.description}</AlertDescription>
        </Alert>

        <Card>
          <CardContent className="space-y-4">
            <div className="space-y-1">
              <p className="text-sm font-medium">Use CLI setup</p>
              <p className="text-sm text-muted-foreground">
                Complete first-run setup directly on the backend host:
              </p>
              <code className="block bg-muted px-2 py-1 text-xs">
                oore setup
              </code>
            </div>
            <div className="space-y-1">
              <p className="text-sm font-medium">Expose backend over HTTPS</p>
              <p className="text-sm text-muted-foreground">
                Use a tunnel and reconnect with the assigned HTTPS URL:
              </p>
              <code className="block bg-muted px-2 py-1 text-xs">
                cloudflared tunnel --url {backendUrl}
              </code>
            </div>
            {hostedUi ? (
              <div className="space-y-1">
                <p className="text-sm font-medium">
                  Use local/self-hosted web UI
                </p>
                <p className="text-sm text-muted-foreground">
                  If backend stays local-only, run the bundled local web
                  launcher:
                </p>
                <code className="block bg-muted px-2 py-1 text-xs">
                  oore-web --backend-url {backendUrl}
                </code>
              </div>
            ) : null}
          </CardContent>
        </Card>

        <div className="flex flex-col gap-2 sm:flex-row">
          <Button onClick={() => void router.invalidate()}>Retry</Button>
          <Button variant="outline" onClick={goBack}>
            Go back
          </Button>
          <Button
            variant="outline"
            onClick={() => window.open('https://docs.oore.build', '_blank')}
          >
            Open docs
          </Button>
        </div>
      </div>
    </div>
  )
}
