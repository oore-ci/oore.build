import { Add01Icon, PlayIcon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { Link } from '@tanstack/react-router'

import type { Build, Pipeline } from '@/lib/types'
import { getStatusVariant } from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { TabsContent } from '@/components/ui/tabs'
import { Spinner } from '@/components/ui/spinner'
import PipelineCard from '@/components/pipeline-card'

export function ProjectBuildsTab({
  builds,
  canTriggerBuild,
  latestSucceededBuild,
  onOpenBuild,
  onPreloadTriggerBuild,
  onTriggerBuild,
  pipelineCount,
  projectHasSource,
}: {
  builds: Array<Build>
  canTriggerBuild: boolean
  latestSucceededBuild: Build | null
  onOpenBuild: (buildId: string) => void
  onPreloadTriggerBuild: () => void
  onTriggerBuild: () => void
  pipelineCount: number
  projectHasSource: boolean
}) {
  return (
    <TabsContent value="builds">
      <div className="pt-2">
        <Card>
          <CardContent>
            {latestSucceededBuild ? (
              <div className="mb-3 flex items-center gap-2 text-sm">
                <Badge variant="secondary" className="text-[10px]">
                  Latest
                </Badge>
                <Link
                  to="/builds/$buildId"
                  params={{ buildId: latestSucceededBuild.id }}
                  className="font-mono text-xs text-primary hover:underline"
                >
                  Build #{latestSucceededBuild.build_number}
                </Link>
                <span className="text-xs text-muted-foreground">
                  on {latestSucceededBuild.branch ?? 'n/a'} ·{' '}
                  {relativeTime(latestSucceededBuild.queued_at)}
                </span>
              </div>
            ) : null}
            {builds.length === 0 ? (
              <Empty className="p-8">
                <EmptyHeader>
                  <EmptyMedia variant="icon">
                    <HugeiconsIcon icon={PlayIcon} />
                  </EmptyMedia>
                  <EmptyTitle>No builds yet</EmptyTitle>
                  <EmptyDescription>
                    {canTriggerBuild
                      ? 'Run this project’s first pipeline to see its status, output, and artifacts here.'
                      : 'Builds will appear here once triggered by a developer.'}
                  </EmptyDescription>
                </EmptyHeader>
                {canTriggerBuild && pipelineCount > 0 && projectHasSource ? (
                  <EmptyContent>
                    <Button
                      size="sm"
                      onMouseEnter={onPreloadTriggerBuild}
                      onFocus={onPreloadTriggerBuild}
                      onClick={onTriggerBuild}
                    >
                      <HugeiconsIcon icon={PlayIcon} />
                      Run first build
                    </Button>
                  </EmptyContent>
                ) : null}
              </Empty>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Build</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Trigger</TableHead>
                    <TableHead>Branch</TableHead>
                    <TableHead>Commit</TableHead>
                    <TableHead>Queued</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {builds.map((build) => (
                    <TableRow
                      key={build.id}
                      className="group cursor-pointer"
                      role="link"
                      tabIndex={0}
                      onClick={() => onOpenBuild(build.id)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' || event.key === ' ') {
                          event.preventDefault()
                          onOpenBuild(build.id)
                        }
                      }}
                    >
                      <TableCell className="font-mono text-sm group-hover:underline">
                        #{build.build_number}
                      </TableCell>
                      <TableCell>
                        <Badge variant={getStatusVariant(build.status)}>
                          {build.status}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline">{build.trigger_type}</Badge>
                      </TableCell>
                      <TableCell className="font-mono text-xs text-muted-foreground">
                        {build.branch ?? 'n/a'}
                      </TableCell>
                      <TableCell className="font-mono text-xs text-muted-foreground">
                        {build.commit_sha
                          ? build.commit_sha.slice(0, 10)
                          : 'n/a'}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {relativeTime(build.queued_at)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      </div>
    </TabsContent>
  )
}

export function ProjectPipelinesTab({
  canTriggerBuild,
  canWritePipelines,
  defaultBranch,
  hasValidRepositoryWorkflow,
  lastBuildByPipeline,
  onPreloadTriggerBuild,
  onTriggerBuild,
  pipelines,
  projectHasSource,
  projectId,
  workflowDiscoveryFailed,
  workflowDiscoveryLoading,
}: {
  canTriggerBuild: boolean
  canWritePipelines: boolean
  defaultBranch: string | undefined
  hasValidRepositoryWorkflow: boolean
  lastBuildByPipeline: Map<string, { status: string; time: number }>
  onPreloadTriggerBuild: () => void
  onTriggerBuild: (pipelineId: string) => void
  pipelines: Array<Pipeline>
  projectHasSource: boolean
  projectId: string
  workflowDiscoveryFailed: boolean
  workflowDiscoveryLoading: boolean
}) {
  return (
    <TabsContent value="pipelines">
      <div className="space-y-4 pt-2">
        {canWritePipelines && pipelines.length > 0 ? (
          <div className="flex justify-end">
            <Button
              size="sm"
              render={
                <Link
                  to="/projects/$projectId/pipelines/new"
                  params={{ projectId }}
                />
              }
            >
              <HugeiconsIcon icon={Add01Icon} />
              Add pipeline
            </Button>
          </div>
        ) : null}

        {pipelines.length === 0 ? (
          <Empty className="border p-8">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                {workflowDiscoveryLoading ? (
                  <Spinner className="size-5" />
                ) : (
                  <HugeiconsIcon icon={Add01Icon} />
                )}
              </EmptyMedia>
              <EmptyTitle>
                {workflowDiscoveryLoading
                  ? 'Checking your repository'
                  : hasValidRepositoryWorkflow
                    ? 'Your repository is ready'
                    : 'Set up your first build'}
              </EmptyTitle>
              <EmptyDescription>
                {!canWritePipelines
                  ? 'Ask a developer or admin to set up the first build.'
                  : workflowDiscoveryLoading
                    ? `Looking for Oore workflows on ${defaultBranch ?? 'the default branch'}...`
                    : workflowDiscoveryFailed
                      ? 'Oore could not inspect the repository. Open setup to retry or continue manually.'
                      : hasValidRepositoryWorkflow
                        ? 'Oore found a checked-in workflow. Review it, name the pipeline, and run your first build.'
                        : 'Choose a clear starter for your app. Advanced build details stay out of the way until you need them.'}
              </EmptyDescription>
            </EmptyHeader>
            {canWritePipelines ? (
              <EmptyContent>
                <Button
                  render={
                    <Link
                      to="/projects/$projectId/pipelines/new"
                      params={{ projectId }}
                    />
                  }
                >
                  <HugeiconsIcon icon={Add01Icon} />
                  {hasValidRepositoryWorkflow
                    ? 'Use repository workflow'
                    : 'Set up a build'}
                </Button>
              </EmptyContent>
            ) : null}
          </Empty>
        ) : (
          pipelines.map((pipeline) => {
            const lastBuild = lastBuildByPipeline.get(pipeline.id)
            return (
              <PipelineCard
                key={pipeline.id}
                pipeline={pipeline}
                projectId={projectId}
                canWrite={canWritePipelines}
                canTriggerBuild={canTriggerBuild && projectHasSource}
                onPreloadTriggerBuild={onPreloadTriggerBuild}
                onTriggerBuild={onTriggerBuild}
                lastBuildStatus={lastBuild?.status}
                lastBuildTime={lastBuild?.time}
              />
            )
          })
        )}
      </div>
    </TabsContent>
  )
}
