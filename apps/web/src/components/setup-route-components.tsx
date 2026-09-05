import { useNavigate, useRouter } from '@tanstack/react-router'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { getConnectivityIssue, isHostedUiOrigin } from '@/lib/connectivity'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useInstanceStore } from '@/stores/instance-store'

function quotePosixShellArgument(value: string): string {
  return `'${value.replaceAll("'", `'"'"'`)}'`
}

export function SetupStepIndicator({
  currentStep,
  steps,
}: {
  currentStep: number
  steps: Array<string>
}) {
  const boundedStep = Math.min(Math.max(currentStep, 0), steps.length)
  const activeIndex = Math.min(boundedStep, steps.length - 1)
  const activeLabel = steps[activeIndex] ?? ''
  const displayedStep = Math.min(boundedStep + 1, steps.length)
  const progress = steps.length === 0 ? 0 : (displayedStep / steps.length) * 100

  return (
    <nav aria-label="Setup progress" className="space-y-2">
      <div className="flex items-center justify-between text-xs sm:hidden">
        <span className="text-muted-foreground">
          Step {displayedStep} of {steps.length}
        </span>
        <span className="font-medium" aria-current="step">
          {activeLabel}
        </span>
      </div>
      <ol
        className="hidden gap-2 text-xs sm:grid"
        style={{
          gridTemplateColumns: `repeat(${steps.length}, minmax(0, 1fr))`,
        }}
      >
        {steps.map((label, index) => {
          const isActive = index === currentStep
          const isCompleted = index < currentStep

          return (
            <li
              key={label}
              className={`truncate text-center ${
                isActive
                  ? 'font-medium text-foreground'
                  : isCompleted
                    ? 'text-muted-foreground'
                    : 'text-muted-foreground/70'
              }`}
              aria-current={isActive ? 'step' : undefined}
            >
              <span className="sr-only">
                {isCompleted ? 'Completed: ' : isActive ? 'Current: ' : ''}
              </span>
              {label}
            </li>
          )
        })}
      </ol>
      <div
        role="progressbar"
        aria-label="Setup completion"
        aria-valuemin={0}
        aria-valuemax={steps.length}
        aria-valuenow={displayedStep}
        className="h-1 overflow-hidden rounded-full bg-muted"
      >
        <div
          className="h-full rounded-full bg-primary transition-[width] duration-200 motion-reduce:transition-none"
          style={{ width: `${progress}%` }}
        />
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
  const backendUrlArgument = quotePosixShellArgument(backendUrl)
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

        <Card size="sm">
          <CardContent className="space-y-4">
            <div className="space-y-1">
              <p className="text-sm font-medium">Use CLI setup</p>
              <p className="text-sm text-muted-foreground">
                Complete first-run setup directly on the backend host:
              </p>
              <code className="block rounded-md bg-muted px-2 py-1 text-xs">
                oore setup
              </code>
            </div>
            <div className="space-y-1">
              <p className="text-sm font-medium">
                Publish through protected ingress
              </p>
              <p className="text-sm text-muted-foreground">
                Use a named tunnel that Cloudflare Access protects. Do not use a
                public Quick Tunnel for Oore.
              </p>
              <code className="block rounded-md bg-muted px-2 py-1 text-xs">
                cloudflared tunnel run &lt;tunnel-name&gt;
              </code>
              <a
                href="https://docs.oore.build/operate/access/cloudflare-access"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-block text-xs text-foreground underline underline-offset-2"
              >
                Open the Cloudflare Access guide
              </a>
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
                <code className="block rounded-md bg-muted px-2 py-1 text-xs">
                  oore-web --backend-url {backendUrlArgument}
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
