import { lazy, Suspense, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight02Icon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'
import type {
  EnvironmentPromotion,
  ProjectEnvironment,
  Release,
} from '@oore/client/models'

import {
  primaryReleaseAction,
  ReleaseActions,
} from '@/components/releases/release-actions'

const TriggerBuildDrawer = lazy(
  () => import('@/components/trigger-build-drawer'),
)
import { ReleaseCalendar } from '@/components/releases/release-calendar'
import { ReleaseFreezeControl } from '@/components/releases/release-freeze'
import { RecordReleaseButton } from '@/components/releases/record-release-dialog'
import {
  ReleaseRollout,
  ReleaseStateBadge,
} from '@/components/releases/release-rollout'
import { PromoteDialog } from '@/components/environments/promote-dialog'
import { RowList } from '@/components/environments/row-list'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Badge } from '@/components/ui/badge'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { Spinner } from '@/components/ui/spinner'
import {
  useDecidePromotion,
  useProjectEnvironments,
} from '@/hooks/use-environments'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  approversSummary,
  getPromotionStateVariant,
  PROMOTION_STATE_LABEL,
  promotableRelease,
  promotionSourceLabel,
  trackNextAction,
  trackRows,
  type TrackNextAction,
  type TrackRow,
} from '@/lib/environments'
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

function PromotionRow({
  approvers,
  onDecide,
  pending,
  promotion,
}: {
  /** Who can approve, shown while the promotion waits. */
  approvers?: string
  onDecide: (approve: boolean) => void
  pending: boolean
  promotion: EnvironmentPromotion
}) {
  const build = promotion.build_number
    ? `Build #${promotion.build_number}`
    : 'Build'
  const version = promotion.version_name
    ? ` · ${promotion.version_name}${promotion.version_code == null ? '' : ` (${promotion.version_code})`}`
    : ''
  const who =
    promotion.state === 'pending' || !promotion.decided_by_email
      ? promotion.requested_by_email
        ? `Requested by ${promotion.requested_by_email}`
        : 'Requested'
      : `${promotion.state === 'rejected' ? 'Rejected' : 'Approved'} by ${promotion.decided_by_email}`
  const note =
    promotion.state === 'failed'
      ? promotion.error
      : (promotion.decision_reason ?? promotion.reason)
  return (
    <Item size="sm" role="listitem" className="flex-wrap gap-y-2">
      <ItemContent className="min-w-56">
        <ItemTitle>
          {build}
          {version}
          <span className="inline-flex items-center gap-1 font-normal text-muted-foreground">
            {promotionSourceLabel(promotion)}
            <HugeiconsIcon icon={ArrowRight02Icon} size={14} />
            {promotion.environment_name}
          </span>
        </ItemTitle>
        <ItemDescription>
          {who} · {relativeTime(promotion.decided_at ?? promotion.created_at)}
          {note ? ` · ${note}` : ''}
        </ItemDescription>
        {approvers ? (
          <ItemDescription>Can approve: {approvers}</ItemDescription>
        ) : null}
      </ItemContent>
      <ItemActions>
        {promotion.can_decide ? (
          <>
            <Button
              size="sm"
              variant="outline"
              disabled={pending}
              onClick={() => onDecide(false)}
            >
              Reject
            </Button>
            <Button size="sm" disabled={pending} onClick={() => onDecide(true)}>
              {pending ? <Spinner data-icon="inline-start" /> : null}
              Approve
            </Button>
          </>
        ) : (
          <Badge variant={getPromotionStateVariant(promotion.state)}>
            {PROMOTION_STATE_LABEL[promotion.state]}
          </Badge>
        )}
      </ItemActions>
    </Item>
  )
}

/** What a track has, in one line: "2.4.0 (141)", or a promotion waiting. */
function TrackVersion({ row }: { row: TrackRow }) {
  const out = row.out
  return (
    <Stack gap={1} className="min-w-0">
      {out ? (
        <span className="text-sm font-medium tabular-nums">
          <ReleaseLink release={out}>{outLabel(out)}</ReleaseLink>
        </span>
      ) : (
        <Text render={<span />}>Nothing out yet</Text>
      )}
      {out ? (
        <Text variant="caption" render={<span />}>
          {out.build_number ? `Build #${out.build_number} · ` : ''}
          {out.promoted_from_release_id ? 'Promoted' : 'Uploaded'}{' '}
          {relativeTime(out.created_at)}
        </Text>
      ) : null}
      {row.coming.map((release) => (
        <Row key={release.id} gap={2} className="min-w-0 flex-wrap">
          <span
            className={cn(textVariants({ variant: 'caption' }), 'tabular-nums')}
          >
            Next:{' '}
            <ReleaseLink release={release}>
              {releaseVersionLabel(release)}
            </ReleaseLink>
          </span>
          <ReleaseStateBadge release={release} />
        </Row>
      ))}
      {row.pending ? (
        <Text variant="caption" render={<span />} className="tabular-nums">
          {row.pending.version_name ?? `Build #${row.pending.build_number}`}{' '}
          waits for approval
        </Text>
      ) : null}
    </Stack>
  )
}

function TrackRowItem({
  action,
  deciding,
  onApprove,
  onPromote,
  row,
}: {
  action: TrackNextAction
  deciding: boolean
  onApprove: () => void
  onPromote: () => void
  row: TrackRow
}) {
  const out = row.out
  const rolling =
    out && ['rolling', 'paused', 'halted'].includes(out.state) ? out : null
  // The release whose other controls sit in the menu beside the action.
  const menuRelease =
    action?.kind === 'release' ? undefined : (row.coming[0] ?? out)
  return (
    <Item role="listitem" className="flex-wrap gap-y-3">
      <ItemContent className="min-w-40 basis-48">
        <ItemTitle>{row.name}</ItemTitle>
        <ItemDescription>
          {row.environment
            ? releaseDestinationLabel(row.environment)
            : 'Not set up as a track'}
        </ItemDescription>
      </ItemContent>
      <ItemContent className="min-w-40 basis-56">
        <TrackVersion row={row} />
      </ItemContent>
      <div className="hidden w-40 md:block">
        {rolling ? <ReleaseRollout release={rolling} /> : null}
      </div>
      {/* A fixed width keeps the action column aligned across rows. */}
      <ItemActions className="ms-auto justify-end sm:w-44">
        {action?.kind === 'approve' ? (
          <Button size="sm" disabled={deciding} onClick={onApprove}>
            {deciding ? <Spinner data-icon="inline-start" /> : null}
            Approve
          </Button>
        ) : action?.kind === 'promote' ? (
          <Button size="sm" variant="outline" onClick={onPromote}>
            Promote
            <HugeiconsIcon icon={ArrowRight02Icon} data-icon="inline-end" />
          </Button>
        ) : action?.kind === 'release' ? (
          <ReleaseActions release={action.release} />
        ) : null}
        {menuRelease ? <ReleaseActions release={menuRelease} menuOnly /> : null}
      </ItemActions>
    </Item>
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
 * An app's releases: each track with what is out and the one thing to do
 * next, promotions waiting for approval, then each version across stores.
 */
export function ProjectReleasesTab({
  projectId,
  canManage = false,
  canOperate = false,
  canRehearse = false,
}: {
  projectId: string
  /** Maintainers promote straight in and record releases. */
  canManage?: boolean
  /** Developers may ask for promotions into tracks that need approval. */
  canOperate?: boolean
  /** The person can run builds here and the app has a pipeline. */
  canRehearse?: boolean
}) {
  const navigate = useNavigate()
  const [rehearseOpen, setRehearseOpen] = useState(false)
  const [promoting, setPromoting] = useState<ProjectEnvironment>()
  const releasesQuery = useReleases({ project_id: projectId, limit: 200 })
  const environmentsQuery = useProjectEnvironments(projectId)
  const decideMutation = useDecidePromotion(projectId)
  const latestBuildQuery = useBuilds({
    project_id: projectId,
    sort: 'created_at',
    direction: 'desc',
    limit: 10,
  })
  const releases = releasesQuery.data?.releases ?? []
  const environments = environmentsQuery.data?.environments ?? []
  const promotions = environmentsQuery.data?.promotions ?? []
  const pendingPromotions = promotions.filter(
    (promotion) => promotion.state === 'pending',
  )
  const recentPromotions = promotions.filter(
    (promotion) => promotion.state !== 'pending',
  )
  // The highest code a recent build made; the next build counts up from it.
  const recentCodes = (latestBuildQuery.data?.builds ?? []).flatMap((build) =>
    build.version_code != null ? [build.version_code] : [],
  )
  const latestVersionCode = recentCodes.length ? Math.max(...recentCodes) : null
  const clash = playVersionCodeClash(releases, latestVersionCode)
  const rows = trackRows(environments, releaseTracks(releases), promotions)
  const versions = releasesByVersion(releases)
  const promotingRelease = promoting ? promotableRelease(promoting) : undefined

  /** Promote is offered toward later tracks in the same store. */
  function canPromoteFrom(environment: ProjectEnvironment | undefined) {
    if (!environment || !promotableRelease(environment)) return false
    const index = environments.indexOf(environment)
    return environments
      .slice(index + 1)
      .some(
        (target) =>
          target.promotable &&
          target.store === environment.store &&
          (canManage || (canOperate && target.approval_required)),
      )
  }

  function decide(promotion: EnvironmentPromotion, approve: boolean) {
    decideMutation.mutate(
      { approve, promotionId: promotion.id },
      {
        onSuccess: (decided) =>
          toast.success(
            approve
              ? `Promoted build #${decided.build_number ?? ''} to ${decided.environment_name}`
              : `Rejected the promotion to ${decided.environment_name}`,
          ),
        onError: (err) => toast.error(getApiErrorMessage(err, {})),
      },
    )
  }

  const loading = releasesQuery.isLoading || environmentsQuery.isLoading
  const error = releasesQuery.error ?? environmentsQuery.error

  return (
    <TabsContent value="releases">
      <Stack gap={8} className="pt-2">
        <Row gap={2} className="flex-wrap justify-end empty:hidden">
          {canRehearse ? (
            <Button
              size="sm"
              variant="outline"
              onClick={() => setRehearseOpen(true)}
            >
              Rehearse a release
            </Button>
          ) : null}
          <ReleaseFreezeControl projectId={projectId} canManage={canManage} />
          {canManage ? <RecordReleaseButton projectId={projectId} /> : null}
        </Row>
        {rehearseOpen ? (
          <Suspense fallback={null}>
            <TriggerBuildDrawer
              open
              onOpenChange={setRehearseOpen}
              fixedProjectId={projectId}
              rehearsal
              title="Rehearse a release"
              description="Try the release first: see what each store would say, without submitting anything."
              onBuildCreated={(buildId) =>
                void navigate({ to: '/builds/$buildId', params: { buildId } })
              }
            />
          </Suspense>
        ) : null}
        {error ? (
          <Alert variant="destructive">
            <AlertDescription className="flex items-center justify-between gap-3">
              <span>Releases could not be loaded.</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void releasesQuery.refetch()
                  void environmentsQuery.refetch()
                }}
              >
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : loading ? (
          <Skeleton className="h-64 w-full" />
        ) : rows.length === 0 ? (
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
            <SettingsSection
              title="Tracks"
              hint="Promoting a build reuses the version the store already has; nothing is rebuilt."
              actions={
                canManage ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    render={
                      <Link
                        to="/projects/$projectId"
                        params={{ projectId }}
                        search={{ tab: 'stores' }}
                      />
                    }
                  >
                    Set up tracks
                  </Button>
                ) : undefined
              }
            >
              <RowList>
                {rows.map((row) => (
                  <TrackRowItem
                    key={row.key}
                    row={row}
                    action={trackNextAction(
                      row,
                      canPromoteFrom(row.environment),
                      primaryReleaseAction,
                    )}
                    deciding={
                      decideMutation.isPending &&
                      decideMutation.variables.promotionId === row.pending?.id
                    }
                    onApprove={() => {
                      if (row.pending) decide(row.pending, true)
                    }}
                    onPromote={() => setPromoting(row.environment)}
                  />
                ))}
              </RowList>
            </SettingsSection>

            {pendingPromotions.length > 0 ? (
              <SettingsSection
                title="Waiting for approval"
                meta={
                  <Badge variant="secondary">{pendingPromotions.length}</Badge>
                }
              >
                <RowList>
                  {pendingPromotions.map((promotion) => (
                    <PromotionRow
                      key={promotion.id}
                      approvers={approversSummary(
                        environments.find(
                          (environment) =>
                            environment.id === promotion.environment_id,
                        )?.approvers,
                      )}
                      promotion={promotion}
                      pending={
                        decideMutation.isPending &&
                        decideMutation.variables.promotionId === promotion.id
                      }
                      onDecide={(approve) => decide(promotion, approve)}
                    />
                  ))}
                </RowList>
              </SettingsSection>
            ) : null}

            {versions.length > 0 ? (
              <SettingsSection title="Versions">
                <SettingsSurface>
                  <ul className="divide-y">
                    {versions.map((group) => (
                      <VersionRow key={group.version} group={group} />
                    ))}
                  </ul>
                </SettingsSurface>
              </SettingsSection>
            ) : null}

            {recentPromotions.length > 0 ? (
              <SettingsSection title="Recent promotions">
                <RowList>
                  {recentPromotions.map((promotion) => (
                    <PromotionRow
                      key={promotion.id}
                      promotion={promotion}
                      pending={false}
                      onDecide={() => undefined}
                    />
                  ))}
                </RowList>
              </SettingsSection>
            ) : null}

            {releases.length > 0 ? (
              <ReleaseCalendar releases={releases} />
            ) : null}
          </>
        )}
      </Stack>
      {promoting && promotingRelease ? (
        <PromoteDialog
          open
          onOpenChange={(open) => {
            if (!open) setPromoting(undefined)
          }}
          canEditNotes={canManage}
          canPromoteDirectly={canManage}
          environments={environments}
          projectId={projectId}
          release={promotingRelease}
          source={promoting}
        />
      ) : null}
    </TabsContent>
  )
}
