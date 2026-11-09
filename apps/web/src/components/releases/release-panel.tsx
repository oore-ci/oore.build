import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react'
import type { Release, ReleaseHistoryEntry } from '@oore/client/models'
import { StoreErrorMessage } from '@/components/build-details/store-wait-alert'
import { Row } from '@/components/layout/row'
import { ReleaseActions } from '@/components/releases/release-actions'
import { PromoteReleaseButton } from '@/components/environments/promote-release-button'
import {
  ReleaseRollout,
  ReleaseStateBadge,
} from '@/components/releases/release-rollout'
import { RolloutSteps } from '@/components/releases/rollout-steps'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from '@/components/ui/empty'
import { Separator } from '@/components/ui/separator'
import { Skeleton } from '@/components/ui/skeleton'
import { useBuild } from '@/hooks/use-builds'
import { useRelease } from '@/hooks/use-releases'
import { plainRefusals, reviewGuidelines } from '@/lib/store-refusals'
import { useTime } from '@/hooks/use-time'
import { relativeTime } from '@/lib/format-utils'
import {
  formatRolloutPercent,
  getReleaseHealthVariant,
  isAdoptedRelease,
  needsStoreAction,
  RELEASE_HEALTH_LABEL,
  releaseConsoleLink,
  releaseDestinationLabel,
  releaseNextStep,
  releaseTransitionLabel,
  releaseVersionLabel,
  releaseWaitSummary,
} from '@/lib/releases'
import { cn } from '@/lib/utils'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'

const TriggerBuildDrawer = lazy(
  () => import('@/components/trigger-build-drawer'),
)

/** Build detail's Release tab: every store release made from this build. */
export function ReleasePanel({
  error,
  isLoading,
  onRetry,
  releases,
  selectedReleaseId,
}: {
  error: Error | null
  isLoading: boolean
  onRetry: () => void
  releases: Array<Release>
  selectedReleaseId?: string
}) {
  if (error) {
    return (
      <Alert variant="destructive">
        <AlertDescription className="flex items-center justify-between gap-3">
          <span>Releases could not be loaded.</span>
          <Button variant="outline" size="sm" onClick={onRetry}>
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    )
  }

  if (isLoading) return <Skeleton className="h-48 w-full" />

  if (releases.length === 0) {
    return (
      <Empty className="border">
        <EmptyHeader>
          <EmptyTitle>Not released</EmptyTitle>
          <EmptyDescription>
            This build has not been sent to a store.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  const ordered = selectedReleaseId
    ? [
        ...releases.filter((release) => release.id === selectedReleaseId),
        ...releases.filter((release) => release.id !== selectedReleaseId),
      ]
    : releases

  return (
    <Stack gap={4}>
      {ordered.map((release) => (
        <ReleaseCard
          key={release.id}
          release={release}
          selected={release.id === selectedReleaseId}
        />
      ))}
    </Stack>
  )
}

function ReleaseCard({
  release,
  selected,
}: {
  release: Release
  selected: boolean
}) {
  const cardRef = useRef<HTMLDivElement>(null)
  // A notification deep link lands on this release, so bring it into view.
  useEffect(() => {
    if (selected) cardRef.current?.scrollIntoView({ block: 'start' })
  }, [selected])
  const nowSeconds = useTime((time) => Math.floor(time / 60_000) * 60)
  // The build page polls the release list; refetch the detail (for its
  // history) only when the list reports a change.
  const detailQuery = useRelease(release.id, { poll: false })
  const { refetch: refetchDetail } = detailQuery
  const seenUpdatedAt = useRef(release.updated_at)
  useEffect(() => {
    if (seenUpdatedAt.current === release.updated_at) return
    seenUpdatedAt.current = release.updated_at
    void refetchDetail()
  }, [release.updated_at, refetchDetail])
  // The detail poll is fresher than the list once it has loaded.
  const current = detailQuery.data?.release ?? release

  return (
    <Card
      ref={cardRef}
      size="sm"
      id={`release-${release.id}`}
      className={cn('scroll-mt-4', selected && 'ring-2 ring-primary/40')}
      aria-current={selected ? 'true' : undefined}
    >
      <CardHeader className="flex flex-row flex-wrap items-start justify-between gap-3">
        <Stack gap={1} className="min-w-0">
          <CardTitle className="flex flex-wrap items-center gap-2">
            {releaseDestinationLabel(current)}
            {isAdoptedRelease(current) ? (
              <Badge
                variant="outline"
                title="Made in the store console, not uploaded by Oore. Oore follows it and its controls work."
              >
                Found in store
              </Badge>
            ) : null}
          </CardTitle>
          <Text variant="mono" className="tabular-nums">
            {releaseVersionLabel(current)}
            {current.package_name ? ` · ${current.package_name}` : ''}
          </Text>
        </Stack>
        <Row gap={1} className="-mt-0.5">
          <PromoteReleaseButton release={current} />
          <ReleaseActions release={current} />
        </Row>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <StoreActionNotice release={current} />
        {isAdoptedRelease(current) ? (
          <Text variant="muted">
            Found in{' '}
            {current.store === 'app_store'
              ? 'App Store Connect'
              : 'Play Console'}{' '}
            {relativeTime(current.created_at)}
            {current.version_code != null
              ? `, version code ${current.version_code}`
              : ''}
            .{' '}
            {current.store === 'google_play' && current.auto_rollout
              ? 'Oore runs its rollout schedule and halts it on crash spikes.'
              : current.store === 'google_play'
                ? 'Oore watches it and changes the rollout only when someone asks, here or in Play Console.'
                : 'Oore follows its review and release state.'}
          </Text>
        ) : null}
        <div className="grid gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,1fr)]">
          <ReleaseRollout release={current} />
          <Stack gap={1}>
            <span className={textVariants({ variant: 'caption' })}>
              Next step
            </span>
            <span className="text-sm">
              {releaseNextStep(current, nowSeconds)}
            </span>
          </Stack>
          {current.health === 'unknown' ? null : (
            <Stack gap={1} className="items-start">
              <span className={textVariants({ variant: 'caption' })}>
                Health
              </span>
              <Badge variant={getReleaseHealthVariant(current.health)}>
                {RELEASE_HEALTH_LABEL[current.health]}
              </Badge>
              {current.crash_rate != null ? (
                <span
                  className={cn(
                    textVariants({ variant: 'caption' }),
                    'tabular-nums',
                  )}
                >
                  Crash rate {formatRolloutPercent(current.crash_rate)}
                  {current.crash_rate_threshold != null
                    ? ` (halts at ${formatRolloutPercent(current.crash_rate_threshold)})`
                    : ''}
                </span>
              ) : null}
            </Stack>
          )}
        </div>
        {current.store === 'google_play' &&
        current.auto_rollout &&
        !['complete', 'failed'].includes(current.state) &&
        current.rollout_fraction > 0 ? (
          <RolloutSteps release={current} />
        ) : null}
        {/* Apple runs App Store and TestFlight releases on its own
            schedule; Oore's rollout steps only drive Google Play. */}
        {(current.store === 'google_play' && !current.auto_rollout) ||
        current.approved_by_email ? (
          <Text variant="caption">
            {current.store === 'google_play' && !current.auto_rollout ? (
              <>No schedule: Oore only changes this rollout when asked.</>
            ) : null}
            {current.approved_by_email
              ? `${current.store === 'google_play' && !current.auto_rollout ? ' · ' : ''}Approved by ${current.approved_by_email}`
              : ''}
          </Text>
        ) : null}
        <Separator />
        <section aria-label="Release history" className="flex flex-col gap-2">
          <h3
            className={cn(textVariants({ variant: 'caption' }), 'font-medium')}
          >
            History
          </h3>
          {detailQuery.error ? (
            <Text variant="caption">History could not be loaded.</Text>
          ) : detailQuery.isLoading ? (
            <Skeleton className="h-16 w-full" />
          ) : (
            <ReleaseHistory history={detailQuery.data?.history ?? []} />
          )}
        </section>
      </CardContent>
    </Card>
  )
}

/** Why a blocked or rejected release is stuck, with the console that fixes it. */
function StoreActionNotice({ release }: { release: Release }) {
  const link = releaseConsoleLink(release)
  if (!needsStoreAction(release)) {
    return link ? (
      <a
        href={link.href}
        target="_blank"
        rel="noreferrer"
        className={cn(
          textVariants({ variant: 'caption' }),
          'w-fit underline underline-offset-2',
        )}
      >
        {link.label}
      </a>
    ) : null
  }
  if (release.state === 'rejected') return <RejectionNotice release={release} />
  const summary = releaseWaitSummary(release)
  // The store's words, put plainly when Oore knows the refusal.
  const storeText = release.store_error ?? release.state_reason
  const refusals = plainRefusals(storeText, release)
  return (
    <Alert variant="default">
      <AlertDescription className="flex flex-col gap-2">
        <span className="font-medium text-foreground">{summary?.title}</span>
        {refusals.length ? (
          <ul className="flex flex-col gap-2">
            {refusals.map((refusal) => (
              <li
                key={refusal.code}
                className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1"
              >
                <span className="min-w-0 flex-1 text-foreground">
                  {refusal.sentence}
                </span>
                {refusal.href && refusal.fix ? (
                  <Button
                    size="sm"
                    variant="outline"
                    nativeButton={false}
                    render={
                      <a href={refusal.href} target="_blank" rel="noreferrer" />
                    }
                  >
                    {refusal.fix}
                  </Button>
                ) : null}
              </li>
            ))}
          </ul>
        ) : release.store_error ? (
          <StoreErrorMessage message={release.store_error} />
        ) : null}
        {refusals.length && !release.store_error ? null : (
          <span className="whitespace-pre-line">{summary?.description}</span>
        )}
        {!refusals.some((refusal) => refusal.href) && link ? (
          <a
            href={link.href}
            target="_blank"
            rel="noreferrer"
            className="w-fit font-medium underline underline-offset-2"
          >
            {link.label}
          </a>
        ) : null}
        {refusals.length && storeText ? (
          <StoreDetails message={storeText} />
        ) : null}
      </AlertDescription>
    </Alert>
  )
}

/** The store's own text, folded away behind the plain sentence. */
function StoreDetails({ message }: { message: string }) {
  return (
    <details className="group text-xs">
      <summary className="w-fit cursor-pointer text-muted-foreground underline-offset-2 hover:underline">
        Details from the store
      </summary>
      <div className="mt-2">
        <StoreErrorMessage message={message} />
      </div>
    </details>
  )
}

/**
 * A release the store rejected in review: the reviewer's words, the
 * guidelines they cite, and the two ways forward.
 */
function RejectionNotice({ release }: { release: Release }) {
  const apple = release.store === 'app_store'
  const reviewer = release.state_reason
  const guidelines = reviewGuidelines(reviewer)
  const buildQuery = useBuild(release.build_id ?? '')
  const [resubmitOpen, setResubmitOpen] = useState(false)
  const build = buildQuery.data?.build
  const prefill = useMemo(
    () =>
      build
        ? {
            project_id: build.project_id,
            pipeline_id: build.pipeline_id,
            branch: build.branch ?? '',
            workflow: build.workflow ?? '',
          }
        : undefined,
    [build],
  )
  return (
    <Alert variant="default" className="border-destructive/40">
      <AlertDescription className="flex flex-col gap-3">
        <span className="font-medium text-foreground">
          {apple ? 'Apple' : 'Google'} rejected {releaseVersionLabel(release)}
        </span>
        {reviewer ? (
          <Stack gap={1}>
            <span className={textVariants({ variant: 'caption' })}>
              {apple ? "Reviewer's message" : "Google's reason"}
              {guidelines.length ? ` · Guideline ${guidelines.join(', ')}` : ''}
            </span>
            <blockquote className="border-l-2 pl-3 whitespace-pre-line text-foreground">
              {reviewer}
            </blockquote>
          </Stack>
        ) : (
          <span>
            {apple ? 'App Store Connect' : 'Play Console'} has the reviewer's
            message.
          </span>
        )}
        <span>
          Reply there if you think the reviewer got it wrong, or fix the app and
          send a new build.
        </span>
        <Row gap={2} className="flex-wrap">
          {release.console_url ? (
            <Button
              size="sm"
              variant="outline"
              nativeButton={false}
              render={
                <a
                  href={release.console_url}
                  target="_blank"
                  rel="noreferrer"
                />
              }
            >
              {apple ? 'Reply in App Store Connect' : 'Open Play Console'}
            </Button>
          ) : null}
          {build ? (
            <Button size="sm" onClick={() => setResubmitOpen(true)}>
              Resubmit with a new build
            </Button>
          ) : null}
        </Row>
        {build ? (
          <Suspense fallback={null}>
            {resubmitOpen ? (
              <TriggerBuildDrawer
                open={resubmitOpen}
                onOpenChange={setResubmitOpen}
                fixedProjectId={build.project_id}
                prefill={prefill}
                title="Resubmit with a new build"
                description="Builds the fix and sends it for review again. Raise the build number if the store already has this one."
              />
            ) : null}
          </Suspense>
        ) : null}
      </AlertDescription>
    </Alert>
  )
}

function ReleaseHistory({ history }: { history: Array<ReleaseHistoryEntry> }) {
  if (history.length === 0) {
    return <Text variant="caption">No history yet.</Text>
  }

  return (
    <ol className="flex flex-col gap-2">
      {history.map((entry) => (
        <li
          key={entry.id}
          className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-0.5 text-xs sm:grid-cols-[minmax(0,14rem)_minmax(0,1fr)_auto]"
        >
          <p className="flex min-w-0 items-center gap-1.5 font-medium">
            {entry.from_state ? (
              <>
                <ReleaseStateBadge
                  release={{ state: entry.to_state, awaiting_approval: false }}
                />
                {releaseTransitionLabel(entry)}
              </>
            ) : (
              'Release created'
            )}
          </p>
          <p className="min-w-0 truncate text-muted-foreground max-sm:order-last max-sm:col-span-2">
            {entry.actor_email ?? 'Oore (automatic)'}
            {entry.reason ? ` · ${entry.reason}` : ''}
          </p>
          <span
            className="text-right text-muted-foreground tabular-nums"
            title={new Date(entry.at * 1000).toLocaleString()}
          >
            {relativeTime(entry.at)}
          </span>
        </li>
      ))}
    </ol>
  )
}
