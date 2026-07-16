import { useState } from 'react'
import {
  Add01Icon,
  InformationCircleIcon,
  PlayIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'

import type { Build, Pipeline } from '@/lib/types'
import {
  BUILD_STATUS_FILTER_OPTIONS,
  getStatusVariant,
} from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
import { useBuilds } from '@/hooks/use-builds'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import type { SortDirection } from '@/components/collection-controls'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
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
import { cn } from '@/lib/utils'

export const PROJECT_BUILD_SORT_OPTIONS = {
  created_at: 'Newest first',
  status: 'Status',
  pipeline_name: 'Pipeline',
  branch: 'Branch',
} as const

export type ProjectBuildSort = keyof typeof PROJECT_BUILD_SORT_OPTIONS

type ProjectBuildSearchUpdates = Partial<{
  direction: SortDirection
  page: number
  pageSize: 20 | 50 | 100
  q: string
  sort: ProjectBuildSort
  status: string
}>

function BranchSearch({
  onValueChange,
  onSearch,
  value,
}: {
  onValueChange: (value: string) => void
  onSearch: (value: string) => void
  value: string
}) {
  const debouncedSearch = useDebouncedCallback(onSearch, 300)

  return (
    <div className="relative w-full sm:max-w-sm">
      <HugeiconsIcon
        icon={Search01Icon}
        className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input
        type="search"
        value={value}
        onChange={(event) => {
          const next = event.target.value
          onValueChange(next)
          debouncedSearch(next)
        }}
        placeholder="Search by branch"
        aria-label="Search project builds by branch"
        className="pl-9"
      />
    </div>
  )
}

function BuildIdentity({ build }: { build: Build }) {
  return (
    <Link
      to="/builds/$buildId"
      params={{ buildId: build.id }}
      className="group block rounded-sm font-mono outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span className="block text-sm group-hover:underline">
        #{build.build_number}
      </span>
      <span className="block text-[11px] text-muted-foreground">
        {build.id.slice(0, 8)}
      </span>
    </Link>
  )
}

export function ProjectBuildsTab({
  active,
  canTriggerBuild,
  onPreloadTriggerBuild,
  onTriggerBuild,
  pipelineCount,
  projectHasSource,
  projectId,
}: {
  active: boolean
  canTriggerBuild: boolean
  onPreloadTriggerBuild: () => void
  onTriggerBuild: () => void
  pipelineCount: number
  projectHasSource: boolean
  projectId: string
}) {
  const search = useSearch({ from: '/projects/$projectId/' })
  const navigate = useNavigate()
  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const buildsQuery = useBuilds(
    {
      project_id: projectId,
      branch: search.q,
      status: search.status,
      sort: search.sort,
      direction: search.direction,
      limit: pageSize,
      offset: page > 1 ? (page - 1) * pageSize : undefined,
    },
    { enabled: active, refetchInterval: 15_000 },
  )
  const builds = buildsQuery.data?.builds ?? []
  const total = buildsQuery.data?.total ?? 0
  const hasFilters = !!search.q || !!search.status
  const [branchQuery, setBranchQuery] = useState(search.q ?? '')
  const [branchSearchReset, setBranchSearchReset] = useState(0)

  function updateSearch(updates: ProjectBuildSearchUpdates) {
    void navigate({
      to: '/projects/$projectId',
      params: { projectId },
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(page, pageSize, buildsQuery.data?.total, (nextPage) => {
    updateSearch({ page: nextPage === 1 ? undefined : nextPage })
  })

  function handleSortChange(
    nextSort: ProjectBuildSort,
    nextDirection: SortDirection,
  ) {
    updateSearch({
      sort: nextSort,
      direction: nextDirection,
      page: undefined,
    })
  }

  function clearFilters() {
    setBranchQuery('')
    setBranchSearchReset((value) => value + 1)
    updateSearch({ q: undefined, status: undefined, page: undefined })
  }

  const showFilteredEmpty =
    !buildsQuery.isLoading && !buildsQuery.error && total === 0 && hasFilters
  const showTrueEmpty =
    !buildsQuery.isLoading && !buildsQuery.error && total === 0 && !hasFilters

  return (
    <TabsContent value="builds">
      {active ? (
        <div className="flex flex-col gap-4 pt-2">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
            <BranchSearch
              key={branchSearchReset}
              value={branchQuery}
              onValueChange={setBranchQuery}
              onSearch={(value) =>
                updateSearch({ q: value.trim() || undefined, page: undefined })
              }
            />
            <div className="grid grid-cols-2 gap-3 sm:ml-auto sm:flex sm:flex-wrap">
              <Button
                variant="ghost"
                size="sm"
                className={cn(
                  'hidden sm:inline-flex',
                  !hasFilters && 'invisible',
                )}
                aria-hidden={!hasFilters}
                tabIndex={hasFilters ? undefined : -1}
                onClick={clearFilters}
              >
                Clear filters
              </Button>
              <Select
                value={search.status ?? 'all'}
                onValueChange={(value) =>
                  updateSearch({
                    status: value && value !== 'all' ? value : undefined,
                    page: undefined,
                  })
                }
                items={BUILD_STATUS_FILTER_OPTIONS}
              >
                <SelectTrigger
                  className="w-full sm:w-40"
                  aria-label="Filter by status"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {Object.entries(BUILD_STATUS_FILTER_OPTIONS).map(
                    ([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ),
                  )}
                </SelectContent>
              </Select>
              <Select
                value={sort}
                onValueChange={(value) =>
                  handleSortChange(value ?? 'created_at', direction)
                }
                items={PROJECT_BUILD_SORT_OPTIONS}
              >
                <SelectTrigger
                  className="w-full sm:hidden"
                  aria-label="Sort project builds"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {Object.entries(PROJECT_BUILD_SORT_OPTIONS).map(
                    ([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ),
                  )}
                </SelectContent>
              </Select>
              {hasFilters ? (
                <Button
                  variant="ghost"
                  size="sm"
                  className="col-span-2 sm:hidden"
                  onClick={clearFilters}
                >
                  Clear filters
                </Button>
              ) : null}
            </div>
          </div>

          {buildsQuery.error ? (
            <Alert variant="destructive">
              <HugeiconsIcon icon={InformationCircleIcon} size={16} />
              <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <span>Failed to load builds: {buildsQuery.error.message}</span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void buildsQuery.refetch()}
                >
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}

          {showTrueEmpty ? (
            <Empty>
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
          ) : null}

          {showFilteredEmpty ? (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={Search01Icon} />
                </EmptyMedia>
                <EmptyTitle>No matching builds</EmptyTitle>
                <EmptyDescription>
                  Change the current filters or clear them to see all builds.
                </EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                <Button variant="outline" onClick={clearFilters}>
                  Clear filters
                </Button>
              </EmptyContent>
            </Empty>
          ) : null}

          {!buildsQuery.error && (buildsQuery.isLoading || total > 0) ? (
            <section aria-label="Project build history" className="min-w-0">
              <div className="divide-y sm:hidden">
                {buildsQuery.isLoading
                  ? Array.from({ length: 5 }, (_, index) => (
                      <div key={index} className="space-y-2 py-4">
                        <Skeleton className="h-5 w-1/3" />
                        <Skeleton className="h-4 w-2/3" />
                      </div>
                    ))
                  : builds.map((build) => (
                      <div key={build.id} className="space-y-2 py-4">
                        <div className="flex items-start justify-between gap-4">
                          <BuildIdentity build={build} />
                          <Badge variant={getStatusVariant(build.status)}>
                            {build.status}
                          </Badge>
                        </div>
                        {build.context?.pipeline_name ? (
                          <p className="truncate text-sm">
                            {build.context.pipeline_name}
                          </p>
                        ) : null}
                        <div className="flex items-center justify-between gap-4 text-xs text-muted-foreground">
                          <span className="truncate font-mono">
                            {build.branch ?? 'No branch'}
                          </span>
                          <span className="shrink-0">
                            {relativeTime(build.created_at)}
                          </span>
                        </div>
                      </div>
                    ))}
              </div>

              <div className="hidden sm:block">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Build</TableHead>
                      <SortableTableHead
                        sort={sort}
                        sortKey="pipeline_name"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Pipeline
                      </SortableTableHead>
                      <SortableTableHead
                        sort={sort}
                        sortKey="status"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Status
                      </SortableTableHead>
                      <TableHead className="hidden lg:table-cell">
                        Trigger
                      </TableHead>
                      <SortableTableHead
                        className="hidden lg:table-cell"
                        sort={sort}
                        sortKey="branch"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Branch
                      </SortableTableHead>
                      <TableHead className="hidden lg:table-cell">
                        Commit
                      </TableHead>
                      <SortableTableHead
                        sort={sort}
                        sortKey="created_at"
                        direction={direction}
                        onSortChange={handleSortChange}
                      >
                        Created
                      </SortableTableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {buildsQuery.isLoading
                      ? Array.from({ length: 5 }, (_, index) => (
                          <TableRow key={index}>
                            <TableCell>
                              <Skeleton className="h-8 w-20" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-8 w-32" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-6 w-20" />
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Skeleton className="h-6 w-24" />
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Skeleton className="h-4 w-24" />
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Skeleton className="h-4 w-20" />
                            </TableCell>
                            <TableCell>
                              <Skeleton className="h-4 w-20" />
                            </TableCell>
                          </TableRow>
                        ))
                      : builds.map((build) => (
                          <TableRow key={build.id}>
                            <TableCell>
                              <BuildIdentity build={build} />
                            </TableCell>
                            <TableCell>
                              <p className="text-sm">
                                {build.context?.pipeline_name ??
                                  'Unknown pipeline'}
                              </p>
                            </TableCell>
                            <TableCell>
                              <Badge variant={getStatusVariant(build.status)}>
                                {build.status}
                              </Badge>
                            </TableCell>
                            <TableCell className="hidden lg:table-cell">
                              <Badge variant="outline">
                                {build.trigger_type}
                              </Badge>
                            </TableCell>
                            <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                              {build.branch ?? 'n/a'}
                            </TableCell>
                            <TableCell className="hidden font-mono text-xs text-muted-foreground lg:table-cell">
                              {build.commit_sha
                                ? build.commit_sha.slice(0, 10)
                                : 'n/a'}
                            </TableCell>
                            <TableCell className="text-sm text-muted-foreground">
                              {relativeTime(build.created_at)}
                            </TableCell>
                          </TableRow>
                        ))}
                  </TableBody>
                </Table>
              </div>

              {!buildsQuery.isLoading ? (
                <CollectionPagination
                  page={page}
                  pageSize={pageSize}
                  total={total}
                  onPageChange={(nextPage) =>
                    updateSearch({
                      page: nextPage > 1 ? nextPage : undefined,
                    })
                  }
                  onPageSizeChange={(nextPageSize) =>
                    updateSearch({
                      pageSize:
                        nextPageSize === 20
                          ? undefined
                          : (nextPageSize as 50 | 100),
                      page: undefined,
                    })
                  }
                />
              ) : null}
            </section>
          ) : null}
        </div>
      ) : null}
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
