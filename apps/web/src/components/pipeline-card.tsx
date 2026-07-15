import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
  Edit02Icon,
  Link01Icon,
  PlayIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type { Pipeline } from '@/lib/types'
import { useUpdatePipeline } from '@/hooks/use-pipelines'
import { relativeTime } from '@/lib/format-utils'
import { getPipelineStatusVariant } from '@/lib/status-variants'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'

interface PipelineCardProps {
  pipeline: Pipeline
  projectId: string
  canWrite: boolean
  canTriggerBuild: boolean
  onPreloadTriggerBuild: () => void
  onTriggerBuild: (pipelineId: string) => void
  lastBuildStatus?: string
  lastBuildTime?: number
}

export default function PipelineCard({
  pipeline,
  projectId,
  canWrite,
  canTriggerBuild,
  onPreloadTriggerBuild,
  onTriggerBuild,
  lastBuildStatus,
  lastBuildTime,
}: PipelineCardProps) {
  const updateMutation = useUpdatePipeline()
  const [detailsOpen, setDetailsOpen] = useState(false)

  function handleToggle() {
    updateMutation.mutate(
      { pipelineId: pipeline.id, data: { enabled: !pipeline.enabled } },
      {
        onSuccess: () =>
          toast.success(
            pipeline.enabled ? 'Pipeline disabled' : 'Pipeline enabled',
          ),
        onError: (err) => toast.error(`Failed: ${err.message}`),
      },
    )
  }

  return (
    <>
      <Card>
        <CardContent>
          {/* Header */}
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0 space-y-2">
              <div className="flex items-center gap-2.5">
                <Link
                  to="/projects/$projectId/pipelines/$pipelineId"
                  params={{ projectId, pipelineId: pipeline.id }}
                  className="text-sm font-semibold hover:underline"
                >
                  {pipeline.name}
                </Link>
                <Badge variant={getPipelineStatusVariant(pipeline.enabled)}>
                  {pipeline.enabled ? 'enabled' : 'disabled'}
                </Badge>
              </div>
              <div className="flex flex-wrap items-center gap-1.5">
                {pipeline.execution_config.platforms.map((p) => (
                  <Badge key={p} variant="outline" className="text-[11px]">
                    {p}
                  </Badge>
                ))}
                {pipeline.trigger_config.events.length > 0
                  ? pipeline.trigger_config.events.map((e) => (
                      <Badge
                        key={e}
                        variant="secondary"
                        className="text-[11px]"
                      >
                        {e}
                      </Badge>
                    ))
                  : null}
              </div>
            </div>
            <span className="shrink-0 pt-0.5 text-xs text-muted-foreground">
              {lastBuildStatus ? (
                <>
                  Last build:{' '}
                  <span className="font-medium">{lastBuildStatus}</span>
                  {lastBuildTime ? ` ${relativeTime(lastBuildTime)}` : ''}
                </>
              ) : (
                'No builds'
              )}
            </span>
          </div>

          {/* Actions */}
          <div className="mt-4 flex flex-wrap items-center gap-2 border-t pt-4">
            {canTriggerBuild ? (
              <Button
                size="sm"
                onMouseEnter={onPreloadTriggerBuild}
                onFocus={onPreloadTriggerBuild}
                onClick={() => onTriggerBuild(pipeline.id)}
              >
                <HugeiconsIcon icon={PlayIcon} />
                Run build
              </Button>
            ) : null}
            {canWrite ? (
              <Button
                size="sm"
                variant="outline"
                render={
                  <Link
                    to="/projects/$projectId/pipelines/$pipelineId/edit"
                    params={{ projectId, pipelineId: pipeline.id }}
                    search={{}}
                  />
                }
                nativeButton={false}
              >
                <HugeiconsIcon icon={Edit02Icon} />
                Edit
              </Button>
            ) : null}
            {canWrite ? (
              <Button
                size="sm"
                variant="outline"
                onClick={handleToggle}
                disabled={updateMutation.isPending}
              >
                {pipeline.enabled ? 'Disable' : 'Enable'}
              </Button>
            ) : null}
            <Button
              size="sm"
              variant="ghost"
              render={
                <Link
                  to="/projects/$projectId/pipelines/$pipelineId"
                  params={{ projectId, pipelineId: pipeline.id }}
                />
              }
              nativeButton={false}
            >
              <HugeiconsIcon icon={Link01Icon} />
              Permalink
            </Button>

            <Button
              size="sm"
              variant="ghost"
              className="ml-auto"
              onClick={() => setDetailsOpen((o) => !o)}
            >
              <HugeiconsIcon
                icon={detailsOpen ? ArrowDown01Icon : ArrowRight01Icon}
              />
              Details
            </Button>
          </div>

          {/* Expandable details */}
          <Collapsible open={detailsOpen} onOpenChange={setDetailsOpen}>
            <CollapsibleTrigger className="sr-only">
              Toggle details
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="mt-4 space-y-4 border-t pt-4 text-xs text-muted-foreground">
                {/* Config */}
                <div>
                  <p className="mb-1 font-medium text-foreground">
                    Configuration
                  </p>
                  <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
                    <dt>Config path</dt>
                    <dd className="font-mono">{pipeline.config_path}</dd>
                    <dt>Resolution</dt>
                    <dd>
                      {pipeline.config_path_explicit
                        ? 'explicit'
                        : 'auto-detect'}
                    </dd>
                    <dt>Flutter version</dt>
                    <dd className="font-mono">
                      {pipeline.execution_config.flutter_version || 'auto'}
                    </dd>
                  </dl>
                </div>

                {/* Trigger */}
                <div>
                  <p className="mb-1 font-medium text-foreground">Triggers</p>
                  <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
                    <dt>Events</dt>
                    <dd>
                      {pipeline.trigger_config.events.length > 0
                        ? pipeline.trigger_config.events.join(', ')
                        : 'all'}
                    </dd>
                    <dt>Branches</dt>
                    <dd className="font-mono">
                      {pipeline.trigger_config.branches.length > 0
                        ? pipeline.trigger_config.branches.join(', ')
                        : 'all'}
                    </dd>
                    <dt>Cancel previous</dt>
                    <dd>
                      {pipeline.concurrency.cancel_previous ? 'yes' : 'no'}
                    </dd>
                    <dt>Max concurrent</dt>
                    <dd>
                      {pipeline.concurrency.max_concurrent ?? 'unlimited'}
                    </dd>
                  </dl>
                </div>

                {/* Execution */}
                <div>
                  <p className="mb-1 font-medium text-foreground">Execution</p>
                  <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
                    <dt>Pre-build</dt>
                    <dd className="font-mono">
                      {pipeline.execution_config.commands.pre_build.length > 0
                        ? pipeline.execution_config.commands.pre_build.join(
                            ' && ',
                          )
                        : 'none'}
                    </dd>
                    <dt>Build</dt>
                    <dd className="font-mono">
                      {pipeline.execution_config.commands.build.length > 0
                        ? pipeline.execution_config.commands.build.join(' && ')
                        : 'none'}
                    </dd>
                    <dt>Post-build</dt>
                    <dd className="font-mono">
                      {pipeline.execution_config.commands.post_build.length > 0
                        ? pipeline.execution_config.commands.post_build.join(
                            ' && ',
                          )
                        : 'none'}
                    </dd>
                    <dt>Artifact patterns</dt>
                    <dd className="font-mono">
                      {pipeline.execution_config.artifact_patterns.join(', ') ||
                        'none'}
                    </dd>
                    {(pipeline.execution_config.env?.length ?? 0) > 0 ? (
                      <>
                        <dt>Env vars</dt>
                        <dd>
                          {pipeline.execution_config.env!.length} configured
                        </dd>
                      </>
                    ) : null}
                  </dl>
                </div>
              </div>
            </CollapsibleContent>
          </Collapsible>
        </CardContent>
      </Card>
    </>
  )
}
