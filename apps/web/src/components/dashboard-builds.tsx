import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  CheckmarkCircle02Icon as CheckCircleIcon,
  Moon02Icon as MoonIcon,
} from '@hugeicons/core-free-icons'

import type { Build } from '@oore/client/models'
import {
  DashboardList,
  DashboardSection,
  DashboardSectionError,
  DashboardViewAll,
} from '@/components/dashboard-section'
import RepositoryAvatar from '@/components/repository-avatar'
import { StatusBadge } from '@/components/status-badge'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import { useTime } from '@/hooks/use-time'
import { formatDuration, relativeTime } from '@/lib/format-utils'
import { LIST_NAV_ITEM } from '@/lib/keyboard-shortcuts'
import { cn } from '@/lib/utils'
import { textVariants } from '@/lib/recipes'

const ACTIVE_PRIORITY = {
  running: 0,
  assigned: 1,
  scheduled: 2,
  queued: 3,
  failed: 4,
  timed_out: 5,
  canceled: 6,
  expired: 7,
  succeeded: 8,
} satisfies Record<Build['status'], number>

/** Where the build is, in a few words: its runner, or why it has none yet. */
function whereText(build: Build): string {
  if (build.status === 'scheduled') return 'Waiting to start'
  if (build.status === 'queued') return 'Waiting for a runner'
  return build.context?.runner_name ?? 'No runner'
}

/** How long it ran, or has been running, and when it last moved. */
function timing(build: Build, nowSeconds: number): string {
  if (build.status === 'running') {
    const startedAt = build.started_at ?? build.created_at
    return `${formatDuration(Math.max(0, nowSeconds - startedAt))} so far`
  }
  if (build.finished_at) {
    const ran =
      build.started_at == null
        ? null
        : formatDuration(build.finished_at - build.started_at)
    const when = relativeTime(build.finished_at)
    return ran ? `${ran} · ${when}` : when
  }
  return `queued ${relativeTime(build.queued_at)}`
}

/**
 * A two-line build row for Home: who and what on the first line, where and
 * when on the second, status on the right. The full Builds page has the
 * column view; here the point is to scan many at once.
 */
function DashboardBuildRow({
  build,
  nowSeconds,
}: {
  build: Build
  nowSeconds: number
}) {
  const projectName = build.context?.project_name ?? build.project_id
  const pipelineName = build.context?.pipeline_name ?? 'Build pipeline'
  const active = ACTIVE_PRIORITY[build.status] < ACTIVE_PRIORITY.failed
  return (
    <div role="listitem">
      <Item
        size="sm"
        className="flex-nowrap rounded-none"
        render={
          <Link
            {...LIST_NAV_ITEM}
            to="/builds/$buildId"
            params={{ buildId: build.id }}
            aria-label={`Open ${projectName} build #${build.build_number}`}
          />
        }
      >
        <ItemMedia>
          <RepositoryAvatar
            fullName={build.context?.repository_full_name ?? projectName}
            avatarUrl={build.context?.project_avatar_url}
            repositoryId={build.context?.repository_id}
            provider={build.context?.repository_provider}
            size="sm"
          />
        </ItemMedia>
        <ItemContent className="min-w-0">
          <ItemTitle className="w-full min-w-0">
            <span className="truncate">{projectName}</span>
            <span
              className={cn(
                textVariants({ variant: 'caption' }),
                'font-normal tabular-nums',
              )}
            >
              #{build.build_number}
            </span>
          </ItemTitle>
          <ItemDescription className="line-clamp-1">
            {pipelineName} · {build.branch ?? 'No branch'}
            {active ? ` · ${whereText(build)}` : ''}
          </ItemDescription>
        </ItemContent>
        <ItemActions className="shrink-0 flex-col items-end gap-1">
          <StatusBadge kind="build" value={build.status} />
          <span
            className={cn(
              textVariants({ variant: 'caption' }),
              'whitespace-nowrap tabular-nums',
            )}
          >
            {timing(build, nowSeconds)}
          </span>
        </ItemActions>
      </Item>
    </div>
  )
}

function QuietEmpty({
  icon,
  title,
  description,
}: {
  icon: typeof CheckCircleIcon
  title: string
  description: string
}) {
  return (
    <Empty className="border py-8">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <HugeiconsIcon icon={icon} />
        </EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}

/** Builds that are running or waiting, running ones first. */
export function DashboardBuildsInProgress({
  builds,
  error,
  isLoading,
  onRetry,
}: {
  builds: Array<Build>
  error?: Error | null
  isLoading: boolean
  onRetry: () => void
}) {
  const nowSeconds = useTime((time) => Math.floor(time / 1000))
  const sorted = [...builds].sort(
    (left, right) =>
      ACTIVE_PRIORITY[left.status] - ACTIVE_PRIORITY[right.status] ||
      right.updated_at - left.updated_at,
  )
  return (
    <DashboardSection
      title="In progress"
      count={isLoading || error ? undefined : sorted.length}
      action={<DashboardViewAll to="/builds" />}
    >
      {error ? (
        <DashboardSectionError label="Builds" onRetry={onRetry} />
      ) : isLoading ? (
        <Skeleton className="h-32 w-full" />
      ) : sorted.length === 0 ? (
        <QuietEmpty
          icon={MoonIcon}
          title="Nothing running"
          description="Builds show here as soon as they are queued."
        />
      ) : (
        <DashboardList>
          {sorted.map((build) => ({
            key: build.id,
            node: <DashboardBuildRow build={build} nowSeconds={nowSeconds} />,
          }))}
        </DashboardList>
      )}
    </DashboardSection>
  )
}

/** The latest finished builds, newest first, so a morning glance shows what happened overnight. */
export function DashboardFinishedBuilds({
  builds,
  error,
  isLoading,
  onRetry,
}: {
  builds: Array<Build>
  error?: Error | null
  isLoading: boolean
  onRetry: () => void
}) {
  const nowSeconds = useTime((time) => Math.floor(time / 1000))
  return (
    <DashboardSection
      title="Finished recently"
      action={<DashboardViewAll to="/builds" />}
    >
      {error ? (
        <DashboardSectionError label="Builds" onRetry={onRetry} />
      ) : isLoading ? (
        <Skeleton className="h-32 w-full" />
      ) : builds.length === 0 ? (
        <QuietEmpty
          icon={CheckCircleIcon}
          title="No finished builds yet"
          description="Run a build to see its result here."
        />
      ) : (
        <DashboardList>
          {builds.map((build) => ({
            key: build.id,
            node: <DashboardBuildRow build={build} nowSeconds={nowSeconds} />,
          }))}
        </DashboardList>
      )}
    </DashboardSection>
  )
}
