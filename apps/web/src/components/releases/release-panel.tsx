import { useEffect, useRef } from 'react'
import type { Release, ReleaseHistoryEntry } from '@oore/client/models'
import { StoreErrorMessage } from '@/components/build-details/store-wait-alert'
import { Row } from '@/components/layout/row'
import { ReleaseActions } from '@/components/releases/release-actions'
import { PromoteReleaseButton } from '@/components/environments/promote-release-button'
import {
  ReleaseRollout,
  ReleaseStateBadge,
} from '@/components/releases/release-rollout'
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
import { useRelease } from '@/hooks/use-releases'
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
        {/* Apple runs App Store and TestFlight releases on its own
            schedule; Oore's rollout steps only drive Google Play. */}
        {current.store === 'google_play' || current.approved_by_email ? (
          <Text variant="caption">
            {current.store === 'google_play' && !current.auto_rollout ? (
              <>No schedule: Oore only changes this rollout when asked.</>
            ) : current.store === 'google_play' ? (
              <>
                Schedule:{' '}
                {(current.policy.steps ?? [])
                  .map(formatRolloutPercent)
                  .join(' → ')}
                {current.policy.interval_hours != null
                  ? `, every ${current.policy.interval_hours}h`
                  : ''}
              </>
            ) : null}
            {current.approved_by_email
              ? `${current.store === 'google_play' ? ' · ' : ''}Approved by ${current.approved_by_email}`
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
  const summary = releaseWaitSummary(release)
  return (
    <Alert variant={release.state === 'rejected' ? 'destructive' : 'default'}>
      <AlertDescription className="flex flex-col gap-2">
        <span className="font-medium text-foreground">{summary?.title}</span>
        {release.store_error ? (
          <StoreErrorMessage message={release.store_error} />
        ) : null}
        <span className="whitespace-pre-line">{summary?.description}</span>
        {link ? (
          <a
            href={link.href}
            target="_blank"
            rel="noreferrer"
            className="w-fit font-medium underline underline-offset-2"
          >
            {link.label}
          </a>
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
