import { Link } from '@tanstack/react-router'
import { Play as PlayIcon, Settings as Setting07Icon } from 'lucide-react'

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
  lastBuildStatus?: string
  onPreloadTriggerBuild: () => void
  onTriggerBuild: (projectId: string) => void
}

export default function ProjectCard({
  canOpenSettings,
  canTriggerBuild,
  project,
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
          <div className="flex min-w-0 items-center gap-2">
            <Link
              to="/projects/$projectId"
              params={{ projectId: project.id }}
              className="min-w-0 truncate text-sm font-semibold hover:underline"
            >
              {project.name}
            </Link>
            <Badge
              variant={
                lastBuildStatus ? getStatusVariant(lastBuildStatus) : 'outline'
              }
              className="shrink-0 text-[10px]"
            >
              {lastBuildStatus ?? 'No builds'}
            </Badge>
          </div>
          {project.description ? (
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {project.description}
            </p>
          ) : null}
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
              <PlayIcon />
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
              <Setting07Icon />
            </Button>
          ) : null}
        </div>
      </CardContent>

      <CardContent className="hidden h-full flex-col gap-4 sm:flex">
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
            <div className="min-w-0 flex-1">
              <div className="flex min-w-0 items-center gap-2">
                <Link
                  to="/projects/$projectId"
                  params={{ projectId: project.id }}
                  className="min-w-0 truncate text-sm font-semibold hover:underline"
                >
                  {project.name}
                </Link>
                <Badge
                  variant={
                    lastBuildStatus
                      ? getStatusVariant(lastBuildStatus)
                      : 'outline'
                  }
                  className="shrink-0 text-[10px]"
                >
                  {lastBuildStatus ?? 'No builds'}
                </Badge>
              </div>
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
              <Setting07Icon />
            </Button>
          ) : null}
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
            <PlayIcon />
            Run build
          </Button>
        ) : null}
      </CardContent>
    </Card>
  )
}
