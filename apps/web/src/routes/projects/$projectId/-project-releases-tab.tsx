import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'
import type { Release } from '@oore/client/models'

import { ReleaseActions } from '@/components/releases/release-actions'
import { ReleaseCalendar } from '@/components/releases/release-calendar'
import { ReleaseFreezeControl } from '@/components/releases/release-freeze'
import { RecordReleaseButton } from '@/components/releases/record-release-dialog'
import { ReleaseStateBadge } from '@/components/releases/release-rollout'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from '@/components/ui/empty'
import { Skeleton } from '@/components/ui/skeleton'
import { TabsContent } from '@/components/ui/tabs'
import { useBuilds } from '@/hooks/use-builds'
import { useReleaseAction, useReleases } from '@/hooks/use-releases'
import { relativeTime } from '@/lib/format-utils'
import {
  formatRolloutPercent,
  releaseDestinationLabel,
  releaseVersionLabel,
} from '@/lib/releases'
import {
  playVersionCodeClash,
  releasableTogether,
  releasesByVersion,
  releaseTracks,
  type TrackStatus,
  type VersionGroup,
} from '@/lib/release-tracks'
import { textVariants } from '@/lib/recipes'
import { toast } from '@/lib/toast'
import { cn } from '@/lib/utils'

const LINK_CLASS = 'underline-offset-4 hover:underline'

function ReleaseLink({
  release,
  children,
}: {
  release: Release
  children: React.ReactNode
}) {
  return (
    <Link
      to="/releases/$releaseId"
      params={{ releaseId: release.id }}
      className={LINK_CLASS}
    >
      {children}
    </Link>
  )
}

/** "2.4.0 (141) at 20%", or the version alone once everyone has it. */
function outLabel(release: Release) {
  const version = releaseVersionLabel(release)
  if (release.state === 'complete') return version
  const share = formatRolloutPercent(release.rollout_fraction)
  if (release.state === 'halted') return `${version}, halted at ${share}`
  if (release.state === 'paused') return `${version}, paused at ${share}`
  return `${version} at ${share}`
}

function TrackRow({ track }: { track: TrackStatus }) {
  const destination = releaseDestinationLabel(track.out ?? track.coming[0])
  return (
    <li className="grid gap-x-4 gap-y-1 py-2.5 sm:grid-cols-[12rem_minmax(0,1fr)]">
      <span className="font-medium">{destination}</span>
      <Stack gap={1} className="min-w-0">
        {track.out ? (
          <span className="tabular-nums">
            <ReleaseLink release={track.out}>{outLabel(track.out)}</ReleaseLink>
          </span>
        ) : (
          <Text variant="muted">Nothing out yet</Text>
        )}
        {track.coming.map((release) => (
          <Row key={release.id} gap={2} className="min-w-0 flex-wrap">
            <span
              className={cn(
                textVariants({ variant: 'caption' }),
                'tabular-nums',
              )}
            >
              Next:{' '}
              <ReleaseLink release={release}>
                {releaseVersionLabel(release)}
              </ReleaseLink>
            </span>
            <ReleaseStateBadge release={release} />
          </Row>
        ))}
      </Stack>
    </li>
  )
}

function ReleaseBothButton({ group }: { group: VersionGroup }) {
  const ready = releasableTogether(group)
  const [open, setOpen] = useState(false)
  const mutation = useReleaseAction()
  if (ready.length === 0) return null

  function releaseAll() {
    void Promise.allSettled(
      ready.map((release) =>
        mutation.mutateAsync({
          action: 'release_now',
          releaseId: release.id,
          body: { reason: null, fraction: null, policy: null },
        }),
      ),
    ).then((results) => {
      const failed = results.filter((result) => result.status === 'rejected')
      if (failed.length) {
        toast.error(
          `${failed.length} of ${ready.length} could not be released. Open each release to see why.`,
        )
      } else {
        toast.success(`Releasing ${group.version} everywhere it was approved`)
      }
      setOpen(false)
    })
  }

  return (
    <>
      <Button size="sm" onClick={() => setOpen(true)}>
        {ready.length === 2 ? 'Release both' : `Release all ${ready.length}`}
      </Button>
      <AlertDialog open={open} onOpenChange={setOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Release {group.version}?</AlertDialogTitle>
            <AlertDialogDescription>
              The stores approved it. People get it on{' '}
              {ready.map(releaseDestinationLabel).join(' and ')} once you
              release it.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={mutation.isPending}>
              Cancel
            </AlertDialogCancel>
            <Button onClick={releaseAll} disabled={mutation.isPending}>
              {mutation.isPending ? 'Releasing…' : 'Release'}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function VersionRow({ group }: { group: VersionGroup }) {
  const buildId = group.releases.find((release) => release.build_id)?.build_id
  return (
    <li className="py-3">
      <Row gap={3} className="flex-wrap justify-between">
        <div className="min-w-0">
          <p className="font-medium tabular-nums">{group.version}</p>
          <Text variant="caption">
            {relativeTime(group.createdAt)}
            {buildId ? (
              <>
                {' · '}
                <Link
                  to="/builds/$buildId"
                  params={{ buildId }}
                  className={LINK_CLASS}
                >
                  Open build
                </Link>
              </>
            ) : null}
          </Text>
        </div>
        <ReleaseBothButton group={group} />
      </Row>
      <ul className="mt-2 flex flex-col gap-1.5">
        {group.releases.map((release) => (
          <li key={release.id}>
            <Row gap={3} className="justify-between">
              <span className="min-w-0 truncate">
                <ReleaseLink release={release}>
                  {releaseDestinationLabel(release)}
                </ReleaseLink>
                <span className={textVariants({ variant: 'caption' })}>
                  {release.version_code != null
                    ? ` · ${release.version_code}`
                    : ''}
                  {release.origin === 'adopted' ? ' · found in store' : ''}
                </span>
              </span>
              <Row gap={2} className="shrink-0">
                <ReleaseStateBadge release={release} />
                <ReleaseActions release={release} />
              </Row>
            </Row>
          </li>
        ))}
      </ul>
    </li>
  )
}

/**
 * An app's releases: where each version is on every store track, then each
 * version across both stores with its controls.
 */
export function ProjectReleasesTab({
  projectId,
  canManage = false,
}: {
  projectId: string
  canManage?: boolean
}) {
  const releasesQuery = useReleases({ project_id: projectId, limit: 200 })
  const latestBuildQuery = useBuilds({
    project_id: projectId,
    sort: 'created_at',
    direction: 'desc',
    limit: 10,
  })
  const releases = releasesQuery.data?.releases ?? []
  // The highest code a recent build made; the next build counts up from it.
  const recentCodes = (latestBuildQuery.data?.builds ?? []).flatMap((build) =>
    build.version_code != null ? [build.version_code] : [],
  )
  const latestVersionCode = recentCodes.length ? Math.max(...recentCodes) : null
  const clash = playVersionCodeClash(releases, latestVersionCode)
  const tracks = releaseTracks(releases)
  const versions = releasesByVersion(releases)

  return (
    <TabsContent value="releases">
      <Stack gap={4} className="pt-2">
        <Row gap={2} className="flex-wrap justify-end empty:hidden">
          <ReleaseFreezeControl projectId={projectId} canManage={canManage} />
          {canManage ? <RecordReleaseButton projectId={projectId} /> : null}
        </Row>
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
          <Skeleton className="h-64 w-full" />
        ) : releases.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyTitle>No releases yet</EmptyTitle>
              <EmptyDescription>
                Turn on publishing in a pipeline, or look for releases made in
                Play Console and App Store Connect with Check stores on Home.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <>
            {clash ? (
              <Alert>
                <HugeiconsIcon icon={InformationCircleIcon} size={16} />
                <AlertDescription>
                  Google Play already has version code {clash.highest}, and the
                  next build would use about {clash.next}. Play refuses a code
                  that is not higher, so raise the pipeline&apos;s build number
                  offset before the next release build.
                </AlertDescription>
              </Alert>
            ) : null}
            <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
              <Card size="sm">
                <CardHeader>
                  <CardTitle>Where each version is</CardTitle>
                </CardHeader>
                <CardContent>
                  <ul className="divide-y">
                    {tracks.map((track) => (
                      <TrackRow key={track.key} track={track} />
                    ))}
                  </ul>
                </CardContent>
              </Card>
              <Card size="sm">
                <CardHeader>
                  <CardTitle>Versions</CardTitle>
                </CardHeader>
                <CardContent>
                  <ul className="divide-y">
                    {versions.map((group) => (
                      <VersionRow key={group.version} group={group} />
                    ))}
                  </ul>
                </CardContent>
              </Card>
            </div>
            <ReleaseCalendar releases={releases} />
          </>
        )}
      </Stack>
    </TabsContent>
  )
}
