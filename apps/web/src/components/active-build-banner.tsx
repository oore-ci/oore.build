import { Link } from '@tanstack/react-router'

import RepositoryAvatar from '@/components/repository-avatar'
import type { Build } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { Item, ItemContent, ItemMedia, ItemTitle } from '@/components/ui/item'
import { getStatusVariant } from '@/lib/status-variants'

function elapsed(startEpoch: number): string {
  const seconds = Math.floor(Date.now() / 1000) - startEpoch
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ${minutes % 60}m`
}

interface ActiveBuildBannerProps {
  build: Build
}

function buildStage(build: Build): string {
  if (build.status === 'queued') return 'Waiting in queue'
  if (build.status === 'scheduled') return 'Scheduled'
  if (build.status === 'assigned') return 'Starting'
  return (
    build.step_results?.find(
      (step) => step.status === 'running' || step.status === 'in_progress',
    )?.name ?? 'Running'
  )
}

export default function ActiveBuildBanner({ build }: ActiveBuildBannerProps) {
  const startTime = build.started_at ?? build.queued_at
  const projectName = build.context?.project_name ?? build.project_id

  /**
     <Item variant="outline" size="sm" 
      render={
        <a href="#">
          <ItemMedia>
            <BadgeCheckIcon className="size-5" />
          </ItemMedia>
          <ItemContent>
            <ItemTitle>Your profile has been verified.</ItemTitle>
          </ItemContent>
          <ItemActions>
            <ChevronRightIcon className="size-4" />
          </ItemActions>
        </a>
      } 
      />
   */

  return (
    <Item
      variant="outline"
      size="xs"
      render={<Link to="/builds/$buildId" params={{ buildId: build.id }} />}
    >
      <ItemMedia>
        <RepositoryAvatar
          fullName={build.context?.repository_full_name ?? projectName}
          avatarUrl={build.context?.project_avatar_url}
          size="sm"
        />
      </ItemMedia>
      <ItemContent className="min-w-0">
        <ItemTitle className="grid w-full grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-3 sm:grid-cols-[minmax(0,1fr)_4rem_7rem_7rem_5.5rem] md:grid-cols-[minmax(0,1fr)_4rem_7rem_7rem_minmax(8rem,12rem)_5.5rem]">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate">{projectName}</span>
          </span>
          <span className="hidden font-mono text-xs font-normal text-muted-foreground sm:inline">
            #{build.build_number}
          </span>
          <span className="hidden truncate font-normal text-muted-foreground sm:inline">
            {build.branch ?? 'No branch'}
          </span>
          <Badge
            variant={getStatusVariant(build.status)}
            className="justify-self-start"
          >
            {build.status}
          </Badge>
          <span className="hidden truncate font-normal text-muted-foreground md:inline">
            {buildStage(build)}
          </span>
          <span className="justify-self-end font-mono text-xs font-normal text-muted-foreground">
            {elapsed(startTime)}
          </span>
        </ItemTitle>
      </ItemContent>
    </Item>
  )
}
