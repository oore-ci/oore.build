import type { Release, ReleaseHistoryEntry } from '@oore/client/models'
import { ReleaseActions } from '@/components/releases/release-actions'
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
  RELEASE_HEALTH_LABEL,
  RELEASE_STORE_LABEL,
  releaseNextStep,
  releaseTransitionLabel,
  releaseVersionLabel,
} from '@/lib/releases'

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
    <div className="flex flex-col gap-4">
      {ordered.map((release) => (
        <ReleaseCard key={release.id} release={release} />
      ))}
    </div>
  )
}

function ReleaseCard({ release }: { release: Release }) {
  const nowSeconds = useTime((time) => Math.floor(time / 60_000) * 60)
  const detailQuery = useRelease(release.id)
  // The detail poll is fresher than the list once it has loaded.
  const current = detailQuery.data?.release ?? release

  return (
    <Card size="sm" id={`release-${release.id}`}>
      <CardHeader className="flex flex-row flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <CardTitle>
            {RELEASE_STORE_LABEL[current.store]} · {current.track}
          </CardTitle>
          <p className="font-mono text-xs text-muted-foreground tabular-nums">
            {releaseVersionLabel(current)}
            {current.package_name ? ` · ${current.package_name}` : ''}
          </p>
        </div>
        <ReleaseActions release={current} />
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,1fr)]">
          <ReleaseRollout release={current} />
          <div className="flex flex-col gap-1">
            <span className="text-xs text-muted-foreground">Next step</span>
            <span className="text-sm">
              {releaseNextStep(current, nowSeconds)}
            </span>
          </div>
          <div className="flex flex-col items-start gap-1">
            <span className="text-xs text-muted-foreground">Health</span>
            <Badge variant={getReleaseHealthVariant(current.health)}>
              {RELEASE_HEALTH_LABEL[current.health]}
            </Badge>
            {current.crash_rate != null ? (
              <span className="text-xs text-muted-foreground tabular-nums">
                Crash rate {formatRolloutPercent(current.crash_rate)}
                {current.crash_rate_threshold != null
                  ? ` (halts at ${formatRolloutPercent(current.crash_rate_threshold)})`
                  : ''}
              </span>
            ) : null}
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          Schedule:{' '}
          {(current.policy.steps ?? []).map(formatRolloutPercent).join(' → ')}
          {current.policy.interval_hours != null
            ? `, every ${current.policy.interval_hours}h`
            : ''}
          {current.approved_by_email
            ? ` · Approved by ${current.approved_by_email}`
            : ''}
        </p>
        <Separator />
        <section aria-label="Release history" className="flex flex-col gap-2">
          <h3 className="text-xs font-medium text-muted-foreground">History</h3>
          {detailQuery.error ? (
            <p className="text-xs text-muted-foreground">
              History could not be loaded.
            </p>
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

function ReleaseHistory({ history }: { history: Array<ReleaseHistoryEntry> }) {
  if (history.length === 0) {
    return <p className="text-xs text-muted-foreground">No history yet.</p>
  }

  return (
    <ol className="flex flex-col gap-3">
      {history.map((entry) => (
        <li key={entry.id} className="flex flex-col gap-0.5">
          <div className="flex items-baseline justify-between gap-2">
            <p className="text-xs font-medium">
              <ReleaseStateBadge
                release={{ state: entry.to_state, awaiting_approval: false }}
              />{' '}
              {releaseTransitionLabel(entry)}
            </p>
            <span
              className="shrink-0 text-xs text-muted-foreground"
              title={new Date(entry.at * 1000).toLocaleString()}
            >
              {relativeTime(entry.at)}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {entry.actor_email ?? 'Oore'}
            {entry.reason ? ` · ${entry.reason}` : ''}
          </p>
        </li>
      ))}
    </ol>
  )
}
