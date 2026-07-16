import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon } from '@hugeicons/core-free-icons'

import type { Pipeline } from '@/lib/types'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Spinner } from '@/components/ui/spinner'
import { TabsContent } from '@/components/ui/tabs'
import PipelineCard from '@/components/pipeline-card'

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
