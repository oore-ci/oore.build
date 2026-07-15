import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { PlayIcon, Setting07Icon } from '@hugeicons/core-free-icons'

import type { Project } from '@/lib/types'
import { getStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import RepositoryAvatar from '@/components/repository-avatar'

interface ProjectCardProps {
  canOpenSettings: boolean
  canTriggerBuild: boolean
  project: Project
  pipelineCount?: number
  lastBuildStatus?: string
  onPreloadTriggerBuild: () => void
  onTriggerBuild: (projectId: string) => void
}

export default function ProjectCard({
  canOpenSettings,
  canTriggerBuild,
  project,
  pipelineCount,
  lastBuildStatus,
  onPreloadTriggerBuild,
  onTriggerBuild,
}: ProjectCardProps) {
  return (
    <Card size="sm" className="group relative sm:h-full">
      <CardContent className="flex items-center gap-3 py-3 sm:hidden">
        {project.repository_full_name ? (
          <RepositoryAvatar
            fullName={project.repository_full_name}
            avatarUrl={project.repository_avatar_url}
            repositoryId={project.repository_id}
            provider={project.repository_provider}
          />
        ) : null}
        <div className="min-w-0 flex-1">
          <Link
            to="/projects/$projectId"
            params={{ projectId: project.id }}
            className="block truncate text-sm font-semibold hover:underline"
          >
            {project.name}
          </Link>
          <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
            {pipelineCount != null ? (
              <span>{pipelineCount} pipelines</span>
            ) : null}
            {lastBuildStatus ? (
              <Badge
                variant={getStatusVariant(lastBuildStatus)}
                className="text-[10px]"
              >
                {lastBuildStatus}
              </Badge>
            ) : (
              <span>No builds</span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {canTriggerBuild ? (
            <Button
              size="icon"
              variant="ghost"
              aria-label={`Run a build for ${project.name}`}
              onMouseEnter={onPreloadTriggerBuild}
              onFocus={onPreloadTriggerBuild}
              onClick={() => onTriggerBuild(project.id)}
            >
              <HugeiconsIcon icon={PlayIcon} />
            </Button>
          ) : null}
          {canOpenSettings ? (
            <Button
              variant="ghost"
              size="icon"
              aria-label={`Open settings for ${project.name}`}
              render={
                <Link
                  to="/projects/$projectId"
                  params={{ projectId: project.id }}
                  search={{ tab: 'settings' }}
                />
              }
              nativeButton={false}
            >
              <HugeiconsIcon icon={Setting07Icon} />
            </Button>
          ) : null}
        </div>
      </CardContent>

      <CardContent className="hidden h-full flex-col gap-3 sm:flex">
        <div className="flex items-start justify-between gap-2">
          <div className="flex min-w-0 items-center gap-3">
            {project.repository_full_name ? (
              <RepositoryAvatar
                fullName={project.repository_full_name}
                avatarUrl={project.repository_avatar_url}
                repositoryId={project.repository_id}
                provider={project.repository_provider}
              />
            ) : null}
            <div className="min-w-0">
              <Link
                to="/projects/$projectId"
                params={{ projectId: project.id }}
                className="text-sm font-semibold hover:underline"
              >
                {project.name}
              </Link>
              {project.description ? (
                <p className="mt-0.5 truncate text-xs text-muted-foreground">
                  {project.description}
                </p>
              ) : null}
            </div>
          </div>
          {canOpenSettings ? (
            <Button
              variant="ghost"
              size="icon"
              className="size-7 shrink-0"
              aria-label={`Open settings for ${project.name}`}
              title={`Open settings for ${project.name}`}
              render={
                <Link
                  to="/projects/$projectId"
                  params={{ projectId: project.id }}
                  search={{ tab: 'settings' }}
                />
              }
              nativeButton={false}
            >
              <HugeiconsIcon icon={Setting07Icon} />
            </Button>
          ) : null}
        </div>

        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {pipelineCount != null ? (
            <span>
              {pipelineCount} pipeline{pipelineCount !== 1 ? 's' : ''}
            </span>
          ) : null}
          {lastBuildStatus ? (
            <Badge
              variant={getStatusVariant(lastBuildStatus)}
              className="text-[10px]"
            >
              {lastBuildStatus}
            </Badge>
          ) : (
            <span className="italic">No builds</span>
          )}
        </div>

        {canTriggerBuild ? (
          <Button
            size="sm"
            variant="outline"
            className="mt-auto w-full"
            onMouseEnter={onPreloadTriggerBuild}
            onFocus={onPreloadTriggerBuild}
            onClick={() => onTriggerBuild(project.id)}
          >
            <HugeiconsIcon icon={PlayIcon} />
            Run build
          </Button>
        ) : null}
      </CardContent>
    </Card>
  )
}
