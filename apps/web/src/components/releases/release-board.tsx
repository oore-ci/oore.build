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
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { focusRingVariants, textVariants } from '@/lib/recipes'
import { cn } from '@/lib/utils'

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
              className={cn(focusRingVariants(), 'rounded-2xl')}
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
        <span className={cn(textVariants({ variant: 'caption' }), 'truncate')}>
          {RELEASE_STORE_LABEL[release.store]} {release.track} ·{' '}
          <span className="tabular-nums">{releaseVersionLabel(release)}</span>
        </span>
      </div>
      <Stack gap={1} className="w-full sm:w-48">
        <ReleaseRollout release={release} />
        <span className={cn(textVariants({ variant: 'caption' }), 'truncate')}>
          {releaseNextStep(release, nowSeconds)}
        </span>
      </Stack>
      <Row gap={3} className="w-full justify-between sm:w-auto sm:min-w-56">
        <Row gap={2}>
          <span
            className={cn(textVariants({ variant: 'caption' }), 'sm:sr-only')}
          >
            Rollout health
          </span>
          <RolloutHealth release={release} storeAnchor={storeAnchor} />
        </Row>
        <ReleaseActions release={release} />
      </Row>
    </div>
  )
}

/**
 * Every release that is still moving or waiting on someone: instance-wide on
 * Home, or one project's on its overview.
 */
export function ReleaseBoard({ projectId }: { projectId?: string } = {}) {
  const nowSeconds = useTime((time) => Math.floor(time / 60_000) * 60)
  const releasesQuery = useReleases(
    { project_id: projectId, state: LIVE_STATE_QUERY, limit: 50 },
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
