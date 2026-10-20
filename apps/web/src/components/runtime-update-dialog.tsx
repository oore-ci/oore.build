import type { ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  ArrowUpRight01Icon,
  Download04Icon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import type { RuntimeReleaseStatus } from '@/hooks/use-runtime-updates'
import { useRuntimeUpdates } from '@/hooks/use-runtime-updates'
import type { CoordinatedUpdateStep } from '@/stores/runtime-update-store'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Spinner } from '@/components/ui/spinner'
import { formatReleaseNotes } from '@/components/runtime-update-utils'
import {
  RuntimeUpdatePreflightChecks,
  backendUpdateReady,
} from '@/components/runtime-update-preflight'

function updateButtonLabel(phase: string | undefined, pending: boolean) {
  if (pending) return 'Starting...'
  if (phase === 'restarting') return 'Restarting...'
  if (phase === 'updating') return 'Updating...'
  if (phase === 'failed') return 'Retry update'
  return 'Update now'
}

const coordinatedStepLabel = {
  idle: 'Update Oore',
  backend: 'Updating backend...',
  frontend: 'Updating frontend...',
  reloading: 'Reloading...',
  failed: 'Retry update',
} satisfies Record<CoordinatedUpdateStep, string>

function RuntimeUpdateCard({
  name,
  release,
  phase,
  failure,
  pending,
  managed,
  unmanagedHint = "Run Oore setup from Terminal to repair this device's managed services. No reinstall is required.",
  locked,
  secondary,
  checks,
  onUpdate,
}: {
  name: string
  release: RuntimeReleaseStatus
  phase?: string
  failure?: string | null
  pending: boolean
  managed: boolean
  unmanagedHint?: string
  locked: boolean
  secondary: boolean
  checks?: ReactNode
  onUpdate: () => void
}) {
  const busy = phase === 'updating' || phase === 'restarting' || pending

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{name}</CardTitle>
        <CardDescription>{release.channel} channel</CardDescription>
        <CardAction>
          <Badge variant="outline">Update available</Badge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-2">
          <span className="font-mono text-xs text-muted-foreground">
            {release.version}
          </span>
          <HugeiconsIcon
            icon={ArrowRight01Icon}
            size={14}
            className="shrink-0 text-muted-foreground"
          />
          <span className="font-mono text-xs font-medium">
            {release.latest_version}
          </span>
        </div>
        {phase === 'failed' && !pending ? (
          <p className="mt-2 text-xs wrap-break-word text-destructive">
            {failure || 'The last update stopped before completion.'}
          </p>
        ) : null}
        {checks}
        {!managed && !checks ? (
          <div className="mt-2 space-y-2 text-xs text-muted-foreground">
            <p>{unmanagedHint}</p>
          </div>
        ) : null}
      </CardContent>
      <CardFooter>
        <Button
          size="sm"
          variant={secondary ? 'outline' : 'default'}
          className="w-full"
          disabled={!managed || busy || locked}
          onClick={onUpdate}
        >
          {busy ? (
            <Spinner />
          ) : (
            <HugeiconsIcon icon={Download04Icon} data-icon="inline-start" />
          )}
          {updateButtonLabel(phase, pending)}
        </Button>
      </CardFooter>
    </Card>
  )
}

export default function RuntimeUpdateDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const updates = useRuntimeUpdates()
  const frontend = updates.frontendRelease.data
  const backend = updates.backendRelease.data
  const frontendAvailable = updates.frontendOffered && !!frontend
  const backendAvailable = updates.backendOffered && !!backend
  const plan = updates.coordinatedPlan
  const step = updates.coordinatedStep
  const coordinatedRunning =
    updates.updateOore.isPending || (step !== 'idle' && step !== 'failed')
  const releases = [
    frontendAvailable ? frontend : null,
    backendAvailable ? backend : null,
  ]
    .filter((release): release is RuntimeReleaseStatus => release !== null)
    .filter(
      (release, index, all) =>
        all.findIndex(
          (candidate) => candidate.release_url === release.release_url,
        ) === index,
    )
  const frontendPhase =
    updates.startFrontendUpdate.data?.phase ?? frontend?.phase
  const backendQueryIsCurrent =
    updates.backendUpdate.dataUpdatedAt >=
    updates.startBackendUpdate.submittedAt
  const backendPhase = backendQueryIsCurrent
    ? updates.backendUpdate.data?.phase
    : (updates.startBackendUpdate.data?.phase ??
      updates.backendUpdate.data?.phase)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Update Oore</DialogTitle>
          <DialogDescription>
            Review what changed before updating this Oore installation.
          </DialogDescription>
        </DialogHeader>

        {plan.kind === 'ready' ? (
          <Card size="sm">
            <CardHeader>
              <CardTitle>Oore {plan.target}</CardTitle>
              <CardDescription>
                {plan.backend && plan.frontend
                  ? 'Updates the backend, waits until it is ready on the new version, then updates the frontend and reloads this page.'
                  : plan.backend
                    ? 'Updates the backend and waits until it is ready on the new version.'
                    : 'Updates the frontend and reloads this page.'}
              </CardDescription>
            </CardHeader>
            <CardFooter>
              <Button
                size="sm"
                className="w-full"
                disabled={coordinatedRunning}
                onClick={() => updates.updateOore.mutate()}
              >
                {coordinatedRunning ? (
                  <Spinner />
                ) : (
                  <HugeiconsIcon
                    icon={Download04Icon}
                    data-icon="inline-start"
                  />
                )}
                {coordinatedStepLabel[step]}
              </Button>
            </CardFooter>
          </Card>
        ) : frontend && backend ? (
          <Alert>
            <AlertTitle>Update each component separately</AlertTitle>
            <AlertDescription>{plan.reason}</AlertDescription>
          </Alert>
        ) : null}

        {step === 'failed' && updates.coordinatedError ? (
          <Alert variant="destructive">
            <AlertTitle>Update stopped</AlertTitle>
            <AlertDescription className="wrap-break-word">
              {updates.coordinatedError}
            </AlertDescription>
          </Alert>
        ) : null}

        <p className="text-xs text-muted-foreground">
          {plan.kind === 'ready'
            ? 'For split deployments, update components one at a time: the backend first, then the frontend, so the interface never runs ahead of the API.'
            : 'Update the backend first, then the frontend, so the interface never runs ahead of the API.'}
        </p>

        <div className="grid gap-3 sm:grid-cols-2">
          {backendAvailable ? (
            <RuntimeUpdateCard
              name="Backend"
              release={backend}
              phase={backendPhase}
              failure={updates.backendUpdate.data?.error}
              pending={updates.startBackendUpdate.isPending}
              managed={backendUpdateReady(
                updates.backendPreflight,
                updates.backendUpdate.data?.managed_service,
              )}
              locked={coordinatedRunning}
              secondary={plan.kind === 'ready'}
              checks={
                <RuntimeUpdatePreflightChecks
                  className="mt-2"
                  preflight={updates.backendPreflight}
                />
              }
              onUpdate={() =>
                updates.startBackendUpdate.mutate(undefined, {
                  onSuccess: () =>
                    toast.success(
                      'Backend update started. Readiness will recover after restart.',
                    ),
                  onError: (error) => toast.error(error.message),
                })
              }
            />
          ) : null}
          {frontendAvailable ? (
            <RuntimeUpdateCard
              name="Frontend"
              release={frontend}
              phase={frontendPhase}
              pending={updates.startFrontendUpdate.isPending}
              failure={frontend.error}
              managed={frontend.managed_service}
              unmanagedHint={frontend.managed_service_reason ?? undefined}
              locked={coordinatedRunning}
              secondary={plan.kind === 'ready'}
              onUpdate={() =>
                updates.startFrontendUpdate.mutate(undefined, {
                  onSuccess: () =>
                    toast.success(
                      'Frontend update started. The UI will reconnect after restart.',
                    ),
                  onError: (error) => toast.error(error.message),
                })
              }
            />
          ) : null}
        </div>

        <div className="flex flex-col gap-4">
          {releases.map((release) => {
            const notes = formatReleaseNotes(release.release_notes)
            return (
              <section
                key={release.release_url}
                className="flex flex-col gap-2"
              >
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <p className="text-sm font-medium">
                      {release.release_name}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      What changed in {release.latest_version}
                    </p>
                  </div>
                  <Badge
                    variant="link"
                    render={
                      <a
                        href={release.changelog_url}
                        aria-label={`Open the full changelog for ${release.latest_version}`}
                        target="_blank"
                        rel="noopener noreferrer"
                      />
                    }
                  >
                    Full changelog
                    <HugeiconsIcon
                      icon={ArrowUpRight01Icon}
                      data-icon="inline-end"
                    />
                  </Badge>
                </div>
                <ScrollArea className="max-h-52 rounded-lg bg-muted/30 ring-1 ring-foreground/10">
                  <p className="p-4 text-sm leading-6 whitespace-pre-wrap">
                    {notes ||
                      'No release notes were published for this version.'}
                  </p>
                </ScrollArea>
              </section>
            )
          })}
        </div>
      </DialogContent>
    </Dialog>
  )
}
