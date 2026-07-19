import { Link } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Plus as Add01Icon, Info as InformationCircleIcon } from 'lucide-react'

import type { Pipeline } from '@/lib/types'
import type { SortDirection } from '@/components/collection-controls'
import { CollectionPagination } from '@/components/collection-controls'
import { CollectionSearchInput } from '@/components/collection-search-input'
import { Alert, AlertDescription } from '@/components/ui/alert'
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
import { Skeleton } from '@/components/ui/skeleton'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import { TabsContent } from '@/components/ui/tabs'
import PipelineCard from '@/components/pipeline-card'

export function ProjectPipelinesTab({
  canTriggerBuild,
  canWritePipelines,
  defaultBranch,
  direction,
  error,
  hasValidRepositoryWorkflow,
  isLoading,
  lastBuildByPipeline,
  onPreloadTriggerBuild,
  onDirectionChange,
  onPageChange,
  onPageSizeChange,
  onQueryChange,
  onRetry,
  onSortChange,
  onTriggerBuild,
  page,
  pageSize,
  pipelines,
  projectHasSource,
  projectId,
  query,
  sort,
  total,
  workflowDiscoveryFailed,
  workflowDiscoveryLoading,
}: {
  canTriggerBuild: boolean
  canWritePipelines: boolean
  defaultBranch: string | undefined
  direction: SortDirection
  error?: string
  hasValidRepositoryWorkflow: boolean
  isLoading: boolean
  lastBuildByPipeline: Map<string, { status: string; time: number }>
  onPreloadTriggerBuild: () => void
  onDirectionChange: (direction: SortDirection) => void
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: 20 | 50 | 100) => void
  onQueryChange: (query: string) => void
  onRetry: () => void
  onSortChange: (sort: 'created_at' | 'name') => void
  onTriggerBuild: (pipelineId: string) => void
  page: number
  pageSize: 20 | 50 | 100
  pipelines: Array<Pipeline>
  projectHasSource: boolean
  projectId: string
  query: string
  sort: 'created_at' | 'name'
  total: number
  workflowDiscoveryFailed: boolean
  workflowDiscoveryLoading: boolean
}) {
  return (
    <TabsContent value="pipelines">
      <div className="space-y-4 pt-2">
        {total > 0 || query ? (
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <CollectionSearchInput
              key={query}
              initialValue={query}
              onSearch={onQueryChange}
              placeholder="Search pipelines"
              ariaLabel="Search pipelines"
            />
            <div className="flex gap-2">
              <NativeSelect
                className="min-w-36 flex-1 sm:flex-none"
                aria-label="Sort pipelines"
                value={sort}
                onChange={(event) =>
                  onSortChange(event.target.value as 'created_at' | 'name')
                }
              >
                <NativeSelectOption value="created_at">
                  Created
                </NativeSelectOption>
                <NativeSelectOption value="name">Name</NativeSelectOption>
              </NativeSelect>
              <NativeSelect
                className="min-w-32 flex-1 sm:flex-none"
                aria-label="Pipeline sort direction"
                value={direction}
                onChange={(event) =>
                  onDirectionChange(event.target.value as SortDirection)
                }
              >
                <NativeSelectOption value="desc">Descending</NativeSelectOption>
                <NativeSelectOption value="asc">Ascending</NativeSelectOption>
              </NativeSelect>
            </div>
          </div>
        ) : null}

        {canWritePipelines && (total > 0 || query) ? (
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
              <DynamicLucideIcon icon={Add01Icon} />
              Add pipeline
            </Button>
          </div>
        ) : null}

        {error ? (
          <Alert variant="destructive">
            <DynamicLucideIcon icon={InformationCircleIcon} />
            <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <span>Failed to load pipelines: {error}</span>
              <Button variant="outline" size="sm" onClick={onRetry}>
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : isLoading ? (
          <div className="space-y-3">
            {Array.from({ length: 3 }, (_, index) => (
              <Skeleton key={index} className="h-28 w-full" />
            ))}
          </div>
        ) : total === 0 && query ? (
          <Empty className="border p-8">
            <EmptyHeader>
              <EmptyTitle>No matching pipelines</EmptyTitle>
              <EmptyDescription>
                No pipeline names match “{query}”.
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button variant="outline" onClick={() => onQueryChange('')}>
                Clear search
              </Button>
            </EmptyContent>
          </Empty>
        ) : total === 0 ? (
          <Empty className="border p-8">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                {workflowDiscoveryLoading ? (
                  <Spinner className="size-5" />
                ) : (
                  <DynamicLucideIcon icon={Add01Icon} />
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
                  <DynamicLucideIcon icon={Add01Icon} />
                  {hasValidRepositoryWorkflow
                    ? 'Use repository workflow'
                    : 'Set up a build'}
                </Button>
              </EmptyContent>
            ) : null}
          </Empty>
        ) : (
          <>
            {pipelines.map((pipeline) => {
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
            })}
            <CollectionPagination
              page={page}
              pageSize={pageSize}
              total={total}
              onPageChange={onPageChange}
              onPageSizeChange={(nextPageSize) => {
                if (
                  nextPageSize === 20 ||
                  nextPageSize === 50 ||
                  nextPageSize === 100
                ) {
                  onPageSizeChange(nextPageSize)
                }
              }}
            />
          </>
        )}
      </div>
    </TabsContent>
  )
}
