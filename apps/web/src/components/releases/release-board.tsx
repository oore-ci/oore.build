import { Link } from '@tanstack/react-router'

import type { ListReleasesResponse, Release } from '@oore/client/models'
import { DashboardList, DashboardSection } from '@/components/dashboard-section'
import { storeHealthAnchor } from '@/components/dashboard-store-health'
import { ReleaseActions } from '@/components/releases/release-actions'
import { ReleaseRollout } from '@/components/releases/release-rollout'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { useReleases, useStoreVitals } from '@/hooks/use-releases'
import { useTime } from '@/hooks/use-time'
import {
  getReleaseHealthVariant,
  LIVE_RELEASE_STATES,
  RELEASE_HEALTH_LABEL,
  RELEASE_STORE_LABEL,
  releaseAppName,
  releaseHealthDetail,
  releaseNextStep,
  releaseVersionLabel,
} from '@/lib/releases'

const LIVE_STATE_QUERY = LIVE_RELEASE_STATES.join(',')

/** Newest first by creation, so acting on a release never reorders the list. */
function selectReleases({ releases }: ListReleasesResponse) {
  return [...releases].sort((a, b) => b.created_at - a.created_at)
}

function RolloutHealth({
  release,
  storeAnchor,
}: {
  release: Release
  storeAnchor?: string
}) {
  const badge = (
    <Badge variant={getReleaseHealthVariant(release.health)}>
      {RELEASE_HEALTH_LABEL[release.health]}
    </Badge>
  )
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          storeAnchor ? (
            <a
              href={`#${storeAnchor}`}
              onClick={(event) => {
                const target = document.getElementById(storeAnchor)
                if (!target) return
                event.preventDefault()
                target.scrollIntoView({ behavior: 'smooth', block: 'center' })
              }}
              aria-label={`Rollout health ${RELEASE_HEALTH_LABEL[release.health]}. Open store health for ${releaseAppName(release)}`}
              className="rounded-2xl outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
            />
          ) : (
            <span tabIndex={0} className="rounded-2xl outline-none" />
          )
        }
      >
        {badge}
      </TooltipTrigger>
      <TooltipContent className="max-w-64">
        {releaseHealthDetail(release)}
        {storeAnchor ? ' Store health covers every version.' : null}
      </TooltipContent>
    </Tooltip>
  )
}

function ReleaseRow({
  nowSeconds,
  release,
  storeAnchor,
}: {
  nowSeconds: number
  release: Release
  storeAnchor?: string
}) {
  return (
    <div
      role="listitem"
      className="flex flex-wrap items-center gap-x-6 gap-y-3 px-4 py-3"
    >
      <div className="flex min-w-0 flex-1 basis-56 flex-col">
        <Link
          to="/releases/$releaseId"
          params={{ releaseId: release.id }}
          className="truncate text-sm font-medium underline-offset-4 hover:underline"
        >
          {releaseAppName(release)}
        </Link>
        <span className="truncate text-xs text-muted-foreground">
          {RELEASE_STORE_LABEL[release.store]} {release.track} ·{' '}
          <span className="tabular-nums">{releaseVersionLabel(release)}</span>
        </span>
      </div>
      <div className="flex w-full flex-col gap-1 sm:w-48">
        <ReleaseRollout release={release} />
        <span className="truncate text-xs text-muted-foreground">
          {releaseNextStep(release, nowSeconds)}
        </span>
      </div>
      <div className="flex w-full items-center justify-between gap-3 sm:w-auto sm:min-w-56">
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground sm:sr-only">
            Rollout health
          </span>
          <RolloutHealth release={release} storeAnchor={storeAnchor} />
        </div>
        <ReleaseActions release={release} />
      </div>
    </div>
  )
}

/** Home's view of every release that is still moving or waiting on someone. */
export function ReleaseBoard() {
  const nowSeconds = useTime((time) => Math.floor(time / 60_000) * 60)
  const releasesQuery = useReleases(
    { state: LIVE_STATE_QUERY, limit: 50 },
    { select: selectReleases },
  )
  const storeAnchors = new Set(
    (useStoreVitals().data?.apps ?? []).map((app) =>
      storeHealthAnchor(app.project_id, app.store),
    ),
  )
  const releases = releasesQuery.data ?? []

  // Nothing live and nothing broken: stay out of the way.
  if (releasesQuery.isSuccess && releases.length === 0) return null

  return (
    <DashboardSection
      title="Releases"
      count={releasesQuery.isSuccess ? releases.length : undefined}
    >
      {releasesQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex items-center justify-between gap-3">
            <span>Releases could not be loaded.</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void releasesQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : releasesQuery.isLoading ? (
        <Skeleton className="h-32 w-full" />
      ) : (
        <DashboardList>
          {releases.map((release) => {
            const anchor = storeHealthAnchor(release.project_id, release.store)
            return {
              key: release.id,
              node: (
                <ReleaseRow
                  nowSeconds={nowSeconds}
                  release={release}
                  storeAnchor={storeAnchors.has(anchor) ? anchor : undefined}
                />
              ),
            }
          })}
        </DashboardList>
      )}
    </DashboardSection>
  )
}
