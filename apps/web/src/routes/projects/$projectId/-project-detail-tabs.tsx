import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon, PlayIcon } from '@hugeicons/core-free-icons'
import { useNavigate, useSearch } from '@tanstack/react-router'

import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import { useBuilds } from '@/hooks/use-builds'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { DataTableSelectFilter } from '@/components/data-table'
import type { SortDirection } from '@/components/data-table-features'
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
import { TabsContent } from '@/components/ui/tabs'
import type { ProjectBuildSort } from './-project-build-sort'
import { ProjectBuildInventory } from './-project-build-inventory'

type ProjectBuildSearchUpdates = Partial<{
  direction: SortDirection
  page: number
  pageSize: 20 | 50 | 100
  q: string
  sort: ProjectBuildSort
  status: string
}>

export function ProjectBuildsTab({
  canTriggerBuild,
  onTriggerBuild,
  pipelineCount,
  projectHasSource,
  projectId,
}: {
  canTriggerBuild: boolean
  onTriggerBuild: () => void
  pipelineCount: number
  projectHasSource: boolean
  projectId: string
}) {
  const search = useSearch({ from: '/projects/$projectId/' })
  const navigate = useNavigate({ from: '/projects/$projectId/' })
  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const buildsQuery = useBuilds(
    {
      project_id: projectId,
      q: search.q,
      status: search.status,
      sort: search.sort,
      direction: search.direction,
      limit: pageSize,
      offset: page > 1 ? (page - 1) * pageSize : undefined,
    },
    { refetchInterval: 15_000 },
  )
  const builds = buildsQuery.data?.builds ?? []
  const total = buildsQuery.data?.total ?? 0
  const hasFilters = !!search.q || !!search.status

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
    updateSearch({ q: undefined, status: undefined, page: undefined })
  }

  const showFilteredEmpty =
    !buildsQuery.isLoading && !buildsQuery.error && total === 0 && hasFilters
  const showTrueEmpty =
    !buildsQuery.isLoading && !buildsQuery.error && total === 0 && !hasFilters

  return (
    <TabsContent value="builds" className="min-h-0">
      <div className="flex h-full min-h-0 flex-col gap-4 pt-2">
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
                <Button size="sm" onClick={onTriggerBuild}>
                  <HugeiconsIcon icon={PlayIcon} />
                  Run first build
                </Button>
              </EmptyContent>
            ) : null}
          </Empty>
        ) : null}

        {!buildsQuery.error &&
        (buildsQuery.isLoading || total > 0 || showFilteredEmpty) ? (
          <ProjectBuildInventory
            builds={builds}
            direction={direction}
            filters={
              <DataTableSelectFilter
                value={search.status ?? 'all'}
                options={BUILD_STATUS_FILTER_OPTIONS}
                onValueChange={(value) =>
                  updateSearch({
                    status: value && value !== 'all' ? value : undefined,
                    page: undefined,
                  })
                }
              />
            }
            activeFilterCount={Number(!!search.status)}
            onClearFilters={hasFilters ? clearFilters : undefined}
            isLoading={buildsQuery.isLoading}
            onPageChange={(nextPage) =>
              updateSearch({ page: nextPage > 1 ? nextPage : undefined })
            }
            onSearch={(value) =>
              updateSearch({ q: value.trim() || undefined, page: undefined })
            }
            onSortChange={handleSortChange}
            page={page}
            pageSize={pageSize}
            query={search.q ?? ''}
            sort={sort}
            total={total}
          />
        ) : null}
      </div>
    </TabsContent>
  )
}
