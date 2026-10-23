import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ChevronRightIcon,
  ServerStack01Icon as ServerIcon,
} from '@hugeicons/core-free-icons'

import RepositoryAvatar from '@/components/repository-avatar'
import { StatusBadge } from '@/components/status-badge'
import { Badge } from '@/components/ui/badge'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { BUILD_PLATFORM_LABELS, getBuildPlatforms } from '@/lib/build-platforms'
import { formatDuration, relativeTime } from '@/lib/format-utils'
import { cn } from '@/lib/utils'
import type { Build } from '@oore/client/models'
import { LIST_NAV_ITEM } from '@/lib/keyboard-shortcuts'

/** How long the build ran, or has been running; `null` before it starts. */
function buildDuration(build: Build): string | null {
  if (build.status === 'running') {
    const startedAt = build.started_at ?? build.created_at
    return formatDuration(
      Math.max(0, Math.floor(Date.now() / 1000) - startedAt),
    )
  }
  if (build.started_at && build.finished_at) {
    return formatDuration(build.finished_at - build.started_at)
  }
  return null
}

/** When the build last moved, phrased for its stage. */
function buildMoment(build: Build): string {
  if (build.finished_at) return `finished ${relativeTime(build.finished_at)}`
  if (build.started_at) return `started ${relativeTime(build.started_at)}`
  return `queued ${relativeTime(build.queued_at)}`
}

/** Which runner has or had the build, or why none has it yet. */
type RunnerState = { runner?: string; text: string }

function runnerState(build: Build): RunnerState {
  if (build.status === 'scheduled') return { text: 'Waiting to start' }
  if (build.status === 'queued') return { text: 'Waiting for runner' }
  const runner = build.context?.runner_name ?? undefined
  return runner ? { runner, text: runner } : { text: 'No runner' }
}

/**
 * One build as a row with fixed columns on wide screens: identity, platforms,
 * runner or queue state, duration and when, status. Narrow screens fold the
 * middle columns into one meta line.
 */
export function BuildItem({
  build,
  inList = false,
}: {
  build: Build
  /** Drop the row's own border when it sits on a shared list surface. */
  inList?: boolean
}) {
  const projectName = build.context?.project_name ?? build.project_id
  const pipelineName = build.context?.pipeline_name ?? 'Build pipeline'
  const commitLabel = build.commit_sha
    ? `#${build.commit_sha.slice(0, 8)}`
    : 'No commit'
  const platforms = getBuildPlatforms(build)
  const runner = runnerState(build)
  const duration = buildDuration(build)
  const moment = buildMoment(build)
  const meta = [
    platforms.map((platform) => BUILD_PLATFORM_LABELS[platform]).join(', '),
    runner.text,
    duration,
    moment,
  ]
    .filter(Boolean)
    .join(' · ')

  return (
    <div role="listitem">
      <Item
        variant={inList ? 'default' : 'outline'}
        size={inList ? 'sm' : 'default'}
        className={cn(
          'min-h-14 flex-nowrap xl:grid xl:grid-cols-[auto_minmax(15rem,1.4fr)_minmax(7rem,0.5fr)_minmax(9rem,0.75fr)_minmax(7rem,auto)_minmax(5.5rem,auto)_1rem] xl:gap-3',
          inList && 'rounded-none',
        )}
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
            <span className="text-xs font-normal text-muted-foreground tabular-nums">
              #{build.build_number}
            </span>
          </ItemTitle>
          <ItemDescription className="line-clamp-1">
            {pipelineName} · {build.branch ?? 'No branch'} ·{' '}
            <span className="font-mono">{commitLabel}</span>
          </ItemDescription>
          <p className="truncate text-xs text-muted-foreground xl:hidden">
            {meta}
          </p>
        </ItemContent>

        <div className="hidden flex-wrap items-center gap-1.5 xl:flex">
          {platforms.map((platform) => (
            <Badge key={platform} variant="secondary">
              {BUILD_PLATFORM_LABELS[platform]}
            </Badge>
          ))}
        </div>

        <div className="hidden min-w-0 items-center gap-1.5 text-xs text-muted-foreground xl:flex">
          {runner.runner ? (
            <HugeiconsIcon icon={ServerIcon} className="size-3.5 shrink-0" />
          ) : null}
          <span
            className={cn('truncate', runner.runner && 'text-foreground')}
            title={runner.text}
          >
            {runner.text}
          </span>
        </div>

        <div className="hidden flex-col items-end text-xs tabular-nums xl:flex">
          <span>{duration ?? '—'}</span>
          <span className="text-muted-foreground">{moment}</span>
        </div>

        <ItemActions className="ml-auto xl:contents">
          <StatusBadge
            kind="build"
            value={build.status}
            className="justify-self-end"
          />
          <HugeiconsIcon
            icon={ChevronRightIcon}
            className="justify-self-end"
            aria-hidden
          />
        </ItemActions>
      </Item>
    </div>
  )
}
