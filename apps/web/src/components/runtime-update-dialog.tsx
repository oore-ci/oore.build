import type { ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  ArrowUpRight01Icon,
  Download04Icon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import type {
  RuntimeReleaseEntry,
  RuntimeReleaseStatus,
} from '@/hooks/use-runtime-updates'
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
import { RelativeTime } from '@/components/relative-time'
import { formatReleaseNotes } from '@/components/runtime-update-utils'
import {
  RuntimeUpdatePreflightChecks,
  backendUpdateReady,
} from '@/components/runtime-update-preflight'
import { Row } from '@/components/layout/row'
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

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
        <Row gap={2}>
          <span className={textVariants({ variant: 'mono' })}>
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
        </Row>
        {phase === 'failed' && !pending ? (
          <p className="mt-2 text-xs wrap-break-word text-destructive">
            {failure || 'The last update stopped before completion.'}
          </p>
        ) : null}
        {checks}
        {!managed && !checks ? (
          <div
            className={cn(
              textVariants({ variant: 'caption' }),
              'mt-2 space-y-2',
            )}
          >
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

interface Changelog {
  installed: string
  target: string
  url: string
  entries: Array<RuntimeReleaseEntry>
}

/**
 * Everything the offered components would gain. A component that is further
 * behind covers more releases, so its range and compare link win; entries from
 * both are merged so split installs see the union.
 */
function changelogFor(statuses: Array<RuntimeReleaseStatus>): Changelog | null {
  const widest = statuses.reduce<RuntimeReleaseStatus | null>(
    (best, status) =>
      !best || (status.releases?.length ?? 0) > (best.releases?.length ?? 0)
        ? status
        : best,
    null,
  )
  if (!widest) return null
  const entries = new Map<string, RuntimeReleaseEntry>()
  for (const status of statuses) {
    const list = status.releases?.length
      ? status.releases
      : [
          {
            version: status.latest_version,
            tag: `v${status.latest_version}`,
            published_at: '',
            release_name: status.release_name,
            release_notes: status.release_notes,
            release_url: status.release_url,
          },
        ]
    for (const entry of list) {
      if (!entries.has(entry.version)) entries.set(entry.version, entry)
    }
  }
  return {
    installed: widest.version,
    target: widest.latest_version,
    url: widest.changelog_url,
    entries: [...entries.values()].sort((a, b) =>
      b.published_at.localeCompare(a.published_at),
    ),
  }
}

function publishedAt(entry: RuntimeReleaseEntry) {
  const millis = Date.parse(entry.published_at)
  return Number.isNaN(millis) ? null : Math.floor(millis / 1000)
}

function ReleaseChangelog({ changelog }: { changelog: Changelog }) {
  const count = changelog.entries.length
  return (
    <section className="flex flex-col gap-2">
      <Row gap={3} className="justify-between">
        <div>
          <p className="text-sm font-medium">What changed</p>
          <Text variant="caption">
            {count === 1
              ? `1 release since ${changelog.installed}`
              : `${count} releases since ${changelog.installed}`}
          </Text>
        </div>
        <Badge
          variant="link"
          render={
            <a
              href={changelog.url}
              aria-label={`Open the full changelog from ${changelog.installed} to ${changelog.target}`}
              target="_blank"
              rel="noopener noreferrer"
            />
          }
        >
          Full changelog
          <HugeiconsIcon icon={ArrowUpRight01Icon} data-icon="inline-end" />
        </Badge>
      </Row>
      <ScrollArea className="max-h-72 rounded-lg bg-muted/30 ring-1 ring-foreground/10">
        <div className="flex flex-col divide-y divide-foreground/10">
          {changelog.entries.map((entry) => {
            const notes = formatReleaseNotes(entry.release_notes)
            const at = publishedAt(entry)
            return (
              <article key={entry.version} className="flex flex-col gap-1 p-4">
                <div className="flex items-baseline justify-between gap-3">
                  <a
                    href={entry.release_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="font-mono text-xs font-medium hover:underline"
                  >
                    {entry.release_name}
                  </a>
                  {at === null ? null : (
                    <RelativeTime
                      at={at}
                      className={cn(
                        textVariants({ variant: 'caption' }),
                        'shrink-0',
                      )}
                    />
                  )}
                </div>
                <p className="text-sm leading-6 whitespace-pre-wrap">
                  {notes || 'No release notes were published for this version.'}
                </p>
              </article>
            )
          })}
        </div>
      </ScrollArea>
    </section>
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
  const changelog = changelogFor(
    [
      frontendAvailable ? frontend : null,
      backendAvailable ? backend : null,
    ].filter((release): release is RuntimeReleaseStatus => release !== null),
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

        <Text variant="caption">
          {plan.kind === 'ready'
            ? 'For split deployments, update components one at a time: the backend first, then the frontend, so the interface never runs ahead of the API.'
            : 'Update the backend first, then the frontend, so the interface never runs ahead of the API.'}
        </Text>

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

        {changelog ? <ReleaseChangelog changelog={changelog} /> : null}
      </DialogContent>
    </Dialog>
  )
}
