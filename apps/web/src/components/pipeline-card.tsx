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
    <Card className="overflow-hidden">
      <CardContent className="p-0">
        {/* Header */}
        <div className="flex items-start justify-between gap-4 p-4">
          <div className="min-w-0 space-y-2">
            <div className="flex items-center gap-2">
              <Link
                to="/projects/$projectId/pipelines/$pipelineId"
                params={{ projectId, pipelineId: pipeline.id }}
                className="text-sm font-semibold hover:underline"
              >
                {pipeline.name}
              </Link>
              <Badge
                variant={getPipelineStatusVariant(pipeline.enabled)}
                className="text-[10px]"
              >
                {pipeline.enabled ? 'enabled' : 'disabled'}
              </Badge>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              {pipeline.execution_config.platforms.map((p) => (
                <Badge key={p} variant="outline" className="font-mono text-[10px]">
                  {p}
                </Badge>
              ))}
              {pipeline.trigger_config.events.length > 0
                ? pipeline.trigger_config.events.map((e) => (
                    <Badge key={e} variant="secondary" className="text-[10px]">
                      {e}
                    </Badge>
                  ))
                : null}
            </div>
          </div>
          <span className="shrink-0 pt-0.5 font-mono text-xs text-muted-foreground">
            {lastBuildStatus ? (
              <>
                <span className="font-medium">{lastBuildStatus}</span>
                {lastBuildTime ? (
                  <span className="text-muted-foreground/60">
                    {' '}
                    {relativeTime(lastBuildTime)}
                  </span>
                ) : ''}
              </>
            ) : (
              <span className="italic opacity-50">No builds</span>
            )}
          </span>
        </div>

        {/* Actions */}
        <div className="flex flex-wrap items-center gap-1.5 border-t px-4 py-2.5">
          {canTriggerBuild ? (
            <Button
              size="sm"
              className="h-7 gap-1.5 text-xs"
              onMouseEnter={onPreloadTriggerBuild}
              onFocus={onPreloadTriggerBuild}
              onClick={() => onTriggerBuild(pipeline.id)}
            >
              <HugeiconsIcon icon={PlayIcon} size={13} />
              Run build
            </Button>
          ) : null}
          {canWrite ? (
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1.5 text-xs"
              render={
                <Link
                  to="/projects/$projectId/pipelines/$pipelineId/edit"
                  params={{ projectId, pipelineId: pipeline.id }}
                  search={{}}
                />
              }
              nativeButton={false}
            >
              <HugeiconsIcon icon={Edit02Icon} size={13} />
              Edit
            </Button>
          ) : null}
          {canWrite ? (
            <Button
              size="sm"
              variant="outline"
              className="h-7 text-xs"
              onClick={handleToggle}
              disabled={updateMutation.isPending}
            >
              {pipeline.enabled ? 'Disable' : 'Enable'}
            </Button>
          ) : null}
          <Button
            size="sm"
            variant="ghost"
            className="h-7 gap-1.5 text-xs"
            render={
              <Link
                to="/projects/$projectId/pipelines/$pipelineId"
                params={{ projectId, pipelineId: pipeline.id }}
              />
            }
            nativeButton={false}
          >
            <HugeiconsIcon icon={Link01Icon} size={13} />
            Permalink
          </Button>

          <Button
            size="sm"
            variant="ghost"
            className="ml-auto h-7 gap-1.5 text-xs"
            onClick={() => setDetailsOpen((o) => !o)}
          >
            <HugeiconsIcon
              icon={detailsOpen ? ArrowDown01Icon : ArrowRight01Icon}
              size={13}
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
            <div className="space-y-4 border-t px-4 py-4 text-xs text-muted-foreground">
              {/* Config */}
              <div>
                <p className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/60">
                  Configuration
                </p>
                <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
                  <dt>Config path</dt>
                  <dd className="font-mono">{pipeline.config_path}</dd>
                  <dt>Resolution</dt>
                  <dd>
                    {pipeline.config_path_explicit ? 'explicit' : 'auto-detect'}
                  </dd>
                  <dt>Flutter version</dt>
                  <dd className="font-mono">
                    {pipeline.execution_config.flutter_version || 'auto'}
                  </dd>
                </dl>
              </div>

              {/* Trigger */}
              <div>
                <p className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/60">
                  Triggers
                </p>
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
                  <dd>{pipeline.concurrency.max_concurrent ?? 'unlimited'}</dd>
                </dl>
              </div>

              {/* Execution */}
              <div>
                <p className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/60">
                  Execution
                </p>
                <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
                  <dt>Pre-build</dt>
                  <dd className="font-mono">
                    {pipeline.execution_config.commands.pre_build.length > 0
                      ? pipeline.execution_config.commands.pre_build.join(' && ')
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
                      ? pipeline.execution_config.commands.post_build.join(' && ')
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
                      <dd>{pipeline.execution_config.env!.length} configured</dd>
                    </>
                  ) : null}
                </dl>
              </div>
            </div>
          </CollapsibleContent>
        </Collapsible>
      </CardContent>
    </Card>
  )
}
