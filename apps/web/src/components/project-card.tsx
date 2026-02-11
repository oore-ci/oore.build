import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { PlayIcon, Setting07Icon } from '@hugeicons/core-free-icons'

import type { Project } from '@/lib/types'
import { getStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'

interface ProjectCardProps {
  project: Project
  pipelineCount?: number
  lastBuildStatus?: string
  onTriggerBuild: (projectId: string) => void
}

export default function ProjectCard({
  project,
  pipelineCount,
  lastBuildStatus,
  onTriggerBuild,
}: ProjectCardProps) {
  return (
    <Card className="group relative">
      <CardContent className="space-y-3">
        <div className="flex items-start justify-between gap-2">
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
          <Button
            variant="ghost"
            size="icon"
            className="size-7 shrink-0"
            render={
              <Link
                to="/projects/$projectId"
                params={{ projectId: project.id }}
              />
            }
          >
            <HugeiconsIcon icon={Setting07Icon} size={14} />
          </Button>
        </div>

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
            <span className="italic">No builds</span>
          )}
        </div>

        <Button
          size="sm"
          variant="outline"
          className="w-full"
          onClick={() => onTriggerBuild(project.id)}
        >
          <HugeiconsIcon icon={PlayIcon} size={14} />
          Run
        </Button>
      </CardContent>
    </Card>
  )
}
