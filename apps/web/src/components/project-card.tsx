import { Link } from '@tanstack/solid-router'
import { PlayIcon, Setting07Icon } from '@hugeicons/core-free-icons'
import type { Project } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { HugeIcon } from '@/components/huge-icon'
import { getStatusVariant } from '@/lib/status-variants'

interface ProjectCardProps {
  project: Project
  pipelineCount?: number
  lastBuildStatus?: string
  onTriggerBuild: (projectId: string) => void
}

export default function ProjectCard(props: ProjectCardProps) {
  return (
    <Card class="group relative">
      <CardContent class="space-y-3">
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0">
            <Link
              to="/projects/$projectId"
              params={{ projectId: props.project.id }}
              class="text-sm font-semibold hover:underline"
            >
              {props.project.name}
            </Link>
            {props.project.description ? (
              <p class="mt-0.5 truncate text-xs text-muted-foreground">
                {props.project.description}
              </p>
            ) : null}
          </div>

          <Link
            to="/projects/$projectId"
            params={{ projectId: props.project.id }}
            class="inline-flex size-7 shrink-0 items-center justify-center border border-transparent hover:border-border hover:bg-muted"
            aria-label={`Open ${props.project.name}`}
          >
            <HugeIcon icon={Setting07Icon} size={14} />
          </Link>
        </div>

        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          {props.pipelineCount != null ? (
            <span>
              {props.pipelineCount} pipeline
              {props.pipelineCount !== 1 ? 's' : ''}
            </span>
          ) : null}

          {props.lastBuildStatus ? (
            <Badge
              variant={getStatusVariant(props.lastBuildStatus)}
              class="text-[10px]"
            >
              {props.lastBuildStatus}
            </Badge>
          ) : (
            <span class="italic">No builds</span>
          )}
        </div>

        <Button
          size="sm"
          variant="outline"
          class="w-full"
          onClick={() => props.onTriggerBuild(props.project.id)}
        >
          <HugeIcon icon={PlayIcon} size={14} />
          Run
        </Button>
      </CardContent>
    </Card>
  )
}
