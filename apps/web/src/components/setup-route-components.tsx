import { useNavigate, useRouter } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Tick02Icon } from '@hugeicons/core-free-icons'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { getConnectivityIssue, isHostedUiOrigin } from '@/lib/connectivity'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { useInstanceStore } from '@/stores/instance-store'
import { cn } from '@/lib/utils'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { stepMarkerVariants } from '@/lib/recipes'
import { FocusedFlowLayout } from '@/components/focused-flow-layout'

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
      <ol className="hidden items-center gap-2 text-xs sm:flex">
        {steps.map((label, index) => {
          const isActive = index === currentStep
          const isCompleted = index < currentStep

          return (
            <li
              key={label}
              className="flex min-w-0 flex-1 items-center gap-2 last:flex-none"
              aria-current={isActive ? 'step' : undefined}
            >
              <span
                aria-hidden="true"
                className={cn(
                  stepMarkerVariants(),
                  isCompleted
                    ? 'border-primary bg-primary text-primary-foreground'
                    : isActive
                      ? 'border-primary text-primary'
                      : 'border-border text-muted-foreground',
                )}
              >
                {isCompleted ? (
                  <HugeiconsIcon
                    icon={Tick02Icon}
                    strokeWidth={2.5}
                    className="size-3"
                  />
                ) : (
                  index + 1
                )}
              </span>
              <span
                className={cn(
                  'truncate',
                  isActive
                    ? 'font-medium text-foreground'
                    : 'text-muted-foreground',
                )}
              >
                <span className="sr-only">
                  {isCompleted ? 'Completed: ' : isActive ? 'Current: ' : ''}
                </span>
                {label}
              </span>
              {index < steps.length - 1 ? (
                <span
                  aria-hidden="true"
                  className={cn(
                    'h-px min-w-3 flex-1',
                    isCompleted ? 'bg-primary' : 'bg-border',
                  )}
                />
              ) : null}
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
        className="h-1 overflow-hidden rounded-full bg-muted sm:sr-only"
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
      <Stack gap={2} className="sm:flex-row">
        <Button onClick={() => void router.invalidate()}>Try again</Button>
        <Button
          variant="outline"
          onClick={() => void navigate({ to: '/setup' })}
        >
          Back to setup
        </Button>
      </Stack>
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
      <FocusedFlowLayout align="center">
        <div className="w-full max-w-xl space-y-4">
          <Alert variant="destructive">
            <AlertTitle>Something went wrong</AlertTitle>
            <AlertDescription>{error.message}</AlertDescription>
          </Alert>
          <Stack gap={2} className="sm:flex-row">
            <Button onClick={() => void router.invalidate()}>Retry</Button>
            <Button variant="outline" onClick={goBack}>
              Go back
            </Button>
          </Stack>
        </div>
      </FocusedFlowLayout>
    )
  }

  return (
    <FocusedFlowLayout align="center">
      <div className="w-full max-w-xl space-y-4">
        <Alert variant="destructive">
          <AlertTitle>{issue.title}</AlertTitle>
          <AlertDescription>{issue.description}</AlertDescription>
        </Alert>

        <Card size="sm">
          <CardContent className="space-y-4">
            <div className="space-y-1">
              <p className="text-sm font-medium">Set up in Terminal</p>
              <Text variant="muted">Run this on the Mac that hosts Oore:</Text>
              <code className="block rounded-md bg-muted px-2 py-1 text-xs">
                oore setup
              </code>
            </div>
            <div className="space-y-1">
              <p className="text-sm font-medium">
                Connect through Cloudflare Access
              </p>
              <Text variant="muted">
                Use a named tunnel that Cloudflare Access protects. Do not use a
                public Quick Tunnel for Oore.
              </Text>
              <code className="block rounded-md bg-muted px-2 py-1 text-xs">
                cloudflared tunnel run &lt;tunnel-name&gt;
              </code>
              <a
                href="https://docs.oore.build/operate/access/cloudflare-access"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-block text-xs text-foreground underline underline-offset-2"
              >
                Cloudflare Access guide
              </a>
            </div>
            {hostedUi ? (
              <div className="space-y-1">
                <p className="text-sm font-medium">Open Oore locally</p>
                <Text variant="muted">For a local instance, run:</Text>
                <code className="block rounded-md bg-muted px-2 py-1 text-xs">
                  oore-web --backend-url {backendUrlArgument}
                </code>
              </div>
            ) : null}
          </CardContent>
        </Card>

        <Stack gap={2} className="sm:flex-row">
          <Button onClick={() => void router.invalidate()}>Retry</Button>
          <Button variant="outline" onClick={goBack}>
            Go back
          </Button>
          <Button
            variant="outline"
            onClick={() => window.open('https://docs.oore.build', '_blank')}
          >
            Documentation
          </Button>
        </Stack>
      </div>
    </FocusedFlowLayout>
  )
}
