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
  project: Project
  pipelineCount?: number
  lastBuildStatus?: string
  onPreloadTriggerBuild: () => void
  onTriggerBuild: (projectId: string) => void
}

export default function ProjectCard({
  project,
  pipelineCount,
  lastBuildStatus,
  onPreloadTriggerBuild,
  onTriggerBuild,
}: ProjectCardProps) {
  return (
    <Card className="group relative flex h-full flex-col overflow-hidden">
      <CardContent className="flex flex-1 flex-col gap-3 p-4">
        {/* Header row */}
        <div className="flex items-start justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2.5">
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
          <Button
            variant="ghost"
            size="icon"
            className="size-6 shrink-0 opacity-0 transition-opacity group-hover:opacity-100"
            aria-label={`Open settings for ${project.name}`}
            render={
              <Link
                to="/projects/$projectId"
                params={{ projectId: project.id }}
              />
            }
            nativeButton={false}
          >
            <HugeiconsIcon icon={Setting07Icon} size={14} />
          </Button>
        </div>

        {/* Meta row */}
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {pipelineCount != null ? (
            <span>
              {pipelineCount} pipeline{pipelineCount !== 1 ? 's' : ''}
            </span>
          ) : null}
          {lastBuildStatus ? (
            <Badge variant={getStatusVariant(lastBuildStatus)} className="text-[10px]">
              {lastBuildStatus}
            </Badge>
          ) : (
            <span className="italic opacity-60">No builds</span>
          )}
        </div>
      </CardContent>

      {/* Footer action */}
      <div className="border-t px-4 py-2.5">
        <Button
          size="sm"
          variant="ghost"
          className="h-7 w-full gap-1.5 text-xs"
          onMouseEnter={onPreloadTriggerBuild}
          onFocus={onPreloadTriggerBuild}
          onClick={() => onTriggerBuild(project.id)}
        >
          <HugeiconsIcon icon={PlayIcon} size={13} />
          Run build
        </Button>
      </div>
    </Card>
  )
}
