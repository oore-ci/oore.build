import { useMemo } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, InformationCircleIcon } from '@hugeicons/core-free-icons'

import type { Pipeline } from '@oore/client/models'
import type { SortDirection } from '@/components/data-table-features'
import {
  DataTable,
  DataTableColumnHeader,
  useDataTable,
  type DataTableColumnDef,
  DataTableRowActionsTrigger,
} from '@/components/data-table'
import {
  dataTableSortingState,
  resolveDataTableSorting,
} from '@/components/data-table-features'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
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
import { useUpdatePipeline } from '@/hooks/use-pipelines'
import { relativeTime } from '@/lib/format-utils'
import { enabledStatus } from '@/lib/status-variants'
import { BUILD_PLATFORM_LABELS } from '@/lib/build-platforms'
import { StatusBadge } from '@/components/status-badge'
import { toast } from '@/lib/toast'
import type { LastPipelineBuild } from './-project-build-summary'
import { splitRowVariants } from '@/lib/recipes'

const PIPELINE_SORTS = ['created_at', 'name'] as const

function PipelineActions({
  canTriggerBuild,
  canWrite,
  onTriggerBuild,
  pipeline,
  projectId,
}: {
  canTriggerBuild: boolean
  canWrite: boolean
  onTriggerBuild: (pipelineId: string) => void
  pipeline: Pipeline
  projectId: string
}) {
  const updateMutation = useUpdatePipeline()

  function togglePipeline() {
    updateMutation.mutate(
      { pipelineId: pipeline.id, data: { enabled: !pipeline.enabled } },
      {
        onSuccess: () =>
          toast.success(
            pipeline.enabled ? 'Pipeline disabled' : 'Pipeline enabled',
          ),
        onError: (error) => toast.error(`Failed: ${error.message}`),
      },
    )
  }

  return (
    <DropdownMenu>
      <DataTableRowActionsTrigger label={`Actions for ${pipeline.name}`} />
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Actions</DropdownMenuLabel>
          <DropdownMenuItem
            render={
              <Link
                to="/projects/$projectId/pipelines/$pipelineId"
                params={{ projectId, pipelineId: pipeline.id }}
              />
            }
          >
            Open pipeline
          </DropdownMenuItem>
          {canTriggerBuild ? (
            <DropdownMenuItem onClick={() => onTriggerBuild(pipeline.id)}>
              Run build
            </DropdownMenuItem>
          ) : null}
        </DropdownMenuGroup>
        {canWrite ? <DropdownMenuSeparator /> : null}
        {canWrite ? (
          <DropdownMenuGroup>
            <DropdownMenuItem
              render={
                <Link
                  to="/projects/$projectId/pipelines/$pipelineId/edit"
                  params={{ projectId, pipelineId: pipeline.id }}
                  search={{}}
                />
              }
            >
              Edit pipeline
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={updateMutation.isPending}
              onClick={togglePipeline}
            >
              {pipeline.enabled ? 'Disable pipeline' : 'Enable pipeline'}
            </DropdownMenuItem>
          </DropdownMenuGroup>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function ProjectPipelinesTab({
  canTriggerBuild,
  canWritePipelines,
  defaultBranch,
  direction,
  error,
  hasValidRepositoryWorkflow,
  isLoading,
  lastBuildByPipeline,
  onPageChange,
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
  lastBuildByPipeline: ReadonlyMap<string, LastPipelineBuild>
  onPageChange: (page: number) => void
  onQueryChange: (query: string) => void
  onRetry: () => void
  onSortChange: (sort: 'created_at' | 'name', direction: SortDirection) => void
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
  const columns = useMemo<Array<DataTableColumnDef<Pipeline>>>(
    () => [
      {
        accessorKey: 'name',
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title="Pipeline" />
        ),
        cell: ({ row }) => (
          <Link
            to="/projects/$projectId/pipelines/$pipelineId"
            params={{ projectId, pipelineId: row.original.id }}
            className="font-medium underline-offset-4 hover:underline"
          >
            {row.original.name}
          </Link>
        ),
      },
      {
        accessorKey: 'enabled',
        header: 'Status',
        cell: ({ row }) => (
          <StatusBadge
            kind="enabled"
            value={enabledStatus(row.original.enabled)}
          />
        ),
        enableSorting: false,
      },
      {
        id: 'platforms',
        header: 'Platforms',
        cell: ({ row }) =>
          row.original.execution_config.platforms
            .map((platform) => BUILD_PLATFORM_LABELS[platform])
            .join(', '),
        enableSorting: false,
      },
      {
        id: 'last_build',
        header: 'Last build',
        cell: ({ row }) => {
          const build = lastBuildByPipeline.get(row.original.id)
          if (!build) {
            return (
              <span className="text-muted-foreground">No recent builds</span>
            )
          }
          return (
            <Link
              to="/builds/$buildId"
              params={{ buildId: build.buildId }}
              className="inline-flex items-center gap-2"
            >
              <StatusBadge kind="build" value={build.status} />
              <span className="text-muted-foreground">
                <span className="font-mono">#{build.buildNumber}</span> ·{' '}
                {relativeTime(build.time)}
              </span>
            </Link>
          )
        },
        enableSorting: false,
      },
      {
        accessorKey: 'created_at',
        header: ({ column }) => (
          <DataTableColumnHeader column={column} title="Created" />
        ),
        cell: ({ row }) => relativeTime(row.original.created_at),
      },
      {
        id: 'actions',
        cell: ({ row }) => (
          <PipelineActions
            canTriggerBuild={canTriggerBuild && projectHasSource}
            canWrite={canWritePipelines}
            onTriggerBuild={onTriggerBuild}
            pipeline={row.original}
            projectId={projectId}
          />
        ),
        enableHiding: false,
        enableSorting: false,
      },
    ],
    [
      canTriggerBuild,
      canWritePipelines,
      lastBuildByPipeline,
      onTriggerBuild,
      projectHasSource,
      projectId,
    ],
  )
  const sorting = useMemo(
    () => dataTableSortingState(sort, direction),
    [direction, sort],
  )
  const table = useDataTable({
    columns,
    data: pipelines,
    getRowId: (pipeline) => pipeline.id,
    state: { sorting },
    onSortingChange: (updater) => {
      const next = resolveDataTableSorting(updater, sorting, PIPELINE_SORTS)
      if (!next) return
      onSortChange(next.sort, next.direction)
    },
  })

  return (
    <TabsContent value="pipelines">
      <div className="space-y-4 pt-2">
        {error ? (
          <Alert variant="destructive">
            <HugeiconsIcon icon={InformationCircleIcon} />
            <AlertDescription className={splitRowVariants()}>
              <span>Failed to load pipelines: {error}</span>
              <Button variant="outline" size="sm" onClick={onRetry}>
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : !isLoading && total === 0 && query ? (
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
        ) : !isLoading && total === 0 ? (
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
                  nativeButton={false}
                  render={
                    <Link
                      to="/projects/$projectId/pipelines/new"
                      params={{ projectId }}
                    />
                  }
                >
                  <HugeiconsIcon icon={Add01Icon} data-icon="inline-start" />
                  {hasValidRepositoryWorkflow
                    ? 'Use repository workflow'
                    : 'Set up a build'}
                </Button>
              </EmptyContent>
            ) : null}
          </Empty>
        ) : (
          <DataTable
            table={table}
            search={{
              value: query,
              onChange: onQueryChange,
              placeholder: 'Search pipelines',
            }}
            actions={
              canWritePipelines ? (
                <Button
                  nativeButton={false}
                  render={
                    <Link
                      to="/projects/$projectId/pipelines/new"
                      params={{ projectId }}
                    />
                  }
                >
                  <HugeiconsIcon icon={Add01Icon} data-icon="inline-start" />
                  Add pipeline
                </Button>
              ) : undefined
            }
            pagination={{ onPageChange, page, pageSize, total }}
            isLoading={isLoading}
            emptyMessage={isLoading ? 'Loading pipelines…' : undefined}
          />
        )}
      </div>
    </TabsContent>
  )
}
