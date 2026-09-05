import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ChevronRightIcon,
  ServerStack01Icon as ServerIcon,
} from '@hugeicons/core-free-icons'

import RepositoryAvatar from '@/components/repository-avatar'
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
import {
  BUILD_STATUS_FILTER_OPTIONS,
  getStatusVariant,
} from '@/lib/status-variants'
import type { Build } from '@oore/client/models'

function buildTiming(build: Build): string {
  if (build.status === 'running') {
    const startedAt = build.started_at ?? build.created_at
    return formatDuration(
      Math.max(0, Math.floor(Date.now() / 1000) - startedAt),
    )
  }

  if (build.started_at && build.finished_at) {
    return formatDuration(build.finished_at - build.started_at)
  }

  return relativeTime(build.updated_at)
}

function waitingContext(build: Build): string | undefined {
  if (build.status === 'scheduled') return 'Waiting to start'
  if (build.status === 'queued') return 'Waiting for runner'

  return undefined
}

export function BuildItem({ build }: { build: Build }) {
  const projectName = build.context?.project_name ?? build.project_id
  const pipelineName = build.context?.pipeline_name ?? 'Build pipeline'
  const commitLabel = build.commit_sha
    ? `#${build.commit_sha.slice(0, 8)}`
    : 'No commit'
  const platforms = getBuildPlatforms(build)
  const runnerName =
    build.status === 'running' || build.status === 'assigned'
      ? build.context?.runner_name
      : undefined
  const queueContext = waitingContext(build)
  const activityContext =
    runnerName ??
    queueContext ??
    relativeTime(build.finished_at ?? build.updated_at)

  return (
    <div role="listitem">
      <Item
        variant="outline"
        size="default"
        className="min-h-16 xl:grid xl:grid-cols-[auto_minmax(15rem,1.4fr)_minmax(7rem,0.5fr)_minmax(9rem,0.75fr)_4.5rem_minmax(5.5rem,auto)_2rem] xl:gap-3"
        render={
          <Link
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
          <ItemTitle>
            {projectName}{' '}
            <span className="font-mono text-xs text-muted-foreground">
              #{build.build_number}
            </span>
          </ItemTitle>
          <ItemDescription className="line-clamp-1">
            {pipelineName} · {build.branch ?? 'No branch'} ·{' '}
            <span className="font-mono">{commitLabel}</span>
          </ItemDescription>
          {platforms.length > 0 ? (
            <div className="flex flex-wrap gap-1.5 xl:hidden">
              {platforms.map((platform) => (
                <Badge key={platform} variant="secondary">
                  {BUILD_PLATFORM_LABELS[platform]}
                </Badge>
              ))}
            </div>
          ) : null}
        </ItemContent>

        <div className="hidden flex-wrap items-center gap-1.5 xl:flex">
          {platforms.map((platform) => (
            <Badge key={platform} variant="secondary">
              {BUILD_PLATFORM_LABELS[platform]}
            </Badge>
          ))}
        </div>

        <div className="hidden min-w-0 items-center xl:flex">
          {runnerName ? (
            <Badge variant="outline" className="max-w-full min-w-0">
              <HugeiconsIcon icon={ServerIcon} data-icon="inline-start" />
              <span className="truncate">{activityContext}</span>
            </Badge>
          ) : (
            <span className="truncate text-xs text-muted-foreground">
              {activityContext}
            </span>
          )}
        </div>

        <ItemActions className="ml-auto xl:contents">
          <span className="hidden justify-self-end font-mono text-xs text-muted-foreground tabular-nums sm:inline">
            {buildTiming(build)}
          </span>
          <Badge
            variant={getStatusVariant(build.status)}
            className="justify-self-end"
          >
            {BUILD_STATUS_FILTER_OPTIONS[build.status]}
          </Badge>
          <HugeiconsIcon icon={ChevronRightIcon} className="justify-self-end" />
        </ItemActions>
      </Item>
    </div>
  )
}
