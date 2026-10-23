import { lazy, Suspense, useState } from 'react'
import { createFileRoute, redirect, useSearch } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon, PlayIcon } from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermissions } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import type { SortDirection } from '@/components/data-table-features'
import { PageMeta } from '@/lib/seo'
import { BuildCollection } from './-build-collection'
import { BuildsEmptyState } from './-builds-empty-state'
import { BuildFilters } from './-build-filters'
import {
  BUILD_TRIGGER_FILTER_OPTIONS,
  isBuildTrigger,
  MAX_ACTOR_FILTER_LENGTH,
  type BuildTrigger,
} from './-build-filter-controls'
import {
  searchDateRangeBounds,
  validSearchDate,
} from '@/components/date-range-filter'
import type { BuildSort } from './-build-sort'
import { useListNavigation } from '@/hooks/use-list-navigation'
import { splitRowVariants } from '@/lib/recipes'

const TriggerBuildDrawer = lazy(
  () => import('@/components/trigger-build-drawer'),
)

interface BuildsSearch {
  actor?: string
  branch?: string
  direction?: SortDirection
  from?: string
  page?: number
  pageSize?: 20 | 50 | 100
  pipeline?: string
  project?: string
  q?: string
  sort?: BuildSort
  status?: string
  to?: string
  trigger?: BuildTrigger
}

const BUILD_TRIGGER_VALUES = Object.keys(BUILD_TRIGGER_FILTER_OPTIONS).filter(
  isBuildTrigger,
)

const BUILD_SORT_VALUES = new Set<BuildSort>([
  'created_at',
  'status',
  'project_name',
  'pipeline_name',
  'branch',
])

function parseSearch(search: SearchInput): BuildsSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const project = searchString(search, 'project')?.trim() ?? ''
  const pipeline = searchString(search, 'pipeline')?.trim() ?? ''
  const statusValue = searchString(search, 'status')
  const status =
    statusValue && statusValue in BUILD_STATUS_FILTER_OPTIONS ? statusValue : ''
  const sort = searchChoice(search, 'sort', BUILD_SORT_VALUES)
  const branch = searchString(search, 'branch')?.trim() ?? ''
  const actorValue = searchString(search, 'actor')?.trim() ?? ''
  // The API rejects longer actors; drop them rather than show an error.
  const actor = actorValue.length > MAX_ACTOR_FILTER_LENGTH ? '' : actorValue
  let from = validSearchDate(searchString(search, 'from'))
  let to = validSearchDate(searchString(search, 'to'))
  // `yyyy-MM-dd` sorts by date, so a reversed range just swaps.
  if (from && to && from > to) [from, to] = [to, from]

  return {
    q: q || undefined,
    project: project || undefined,
    pipeline: pipeline || undefined,
    status: status && status !== 'all' ? status : undefined,
    trigger: searchChoice(search, 'trigger', BUILD_TRIGGER_VALUES),
    branch: branch || undefined,
    actor: actor || undefined,
    from,
    to,
    sort,
    direction: searchString(search, 'direction') === 'asc' ? 'asc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/builds/')({
  validateSearch: parseSearch,
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id, context)
    if (useAuthStore.getState().user?.role === 'qa_viewer') {
      throw redirect({ to: '/' })
    }
  },
  component: OperationsBuildsPage,
})

function OperationsBuildsPage() {
  useListNavigation()
  const [buildDrawerOpen, setBuildDrawerOpen] = useState(false)
  const search = useSearch({ from: '/builds/' })
  const navigate = Route.useNavigate()
  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const createdBounds = searchDateRangeBounds(search.from, search.to)
  const buildsQuery = useBuilds({
    q: search.q,
    project_id: search.project,
    pipeline_id: search.pipeline,
    status: search.status,
    trigger: search.trigger,
    branch: search.branch,
    actor: search.actor,
    created_after: createdBounds.after,
    created_before: createdBounds.before,
    sort,
    direction,
    limit: pageSize,
    offset: (page - 1) * pageSize,
  })
  const projectsQuery = useProjects({ limit: 1 })
  const setupStatusQuery = useSetupStatus()
  const [canTriggerBuildGlobally, canWriteProjects] = useHasPermissions([
    'builds:write',
    'projects:write',
  ])
  const builds = buildsQuery.data?.builds ?? []
  const hasProjects = (projectsQuery.data?.total ?? 0) > 0
  const total = buildsQuery.data?.total ?? 0
  const canTriggerBuild = canTriggerBuildGlobally && hasProjects
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const missingProjects =
    !projectsQuery.isLoading && !projectsQuery.error && !hasProjects
  const activeFilterCount = [
    search.project,
    search.pipeline,
    search.status,
    search.trigger,
    search.branch,
    search.actor,
    search.from || search.to,
  ].filter(Boolean).length
  const hasFilters = !!search.q || activeFilterCount > 0
  const branchSuggestions = [
    ...new Set(builds.flatMap((build) => (build.branch ? [build.branch] : []))),
  ]
  const buildCapabilities = {
    triggerBuild: canTriggerBuild,
    writeProjects: canWriteProjects,
  }

  function updateSearch(updates: Partial<BuildsSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(page, pageSize, buildsQuery.data?.total, (nextPage) => {
    updateSearch({ page: nextPage === 1 ? undefined : nextPage })
  })

  function handleSortChange(nextSort: BuildSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  return (
    <PageLayout width="wide" fill>
      <PageMeta title="Builds" noindex />
      <PageHeader
        title="Builds"
        actions={
          canTriggerBuild ? (
            <Suspense fallback={null}>
              <TriggerBuildDrawer
                description="Choose a project and pipeline to run a manual build."
                open={buildDrawerOpen}
                onOpenChange={setBuildDrawerOpen}
                onBuildCreated={(buildId) => {
                  void navigate({
                    to: '/builds/$buildId',
                    params: { buildId },
                  })
                }}
              >
                <Button>
                  <HugeiconsIcon icon={PlayIcon} />
                  Run build
                </Button>
              </TriggerBuildDrawer>
            </Suspense>
          ) : undefined
        }
      />

      {projectsQuery.error ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} />
          <AlertDescription className={splitRowVariants()}>
            <span>
              Project filters and build actions are temporarily unavailable.
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void projectsQuery.refetch()}
            >
              Retry projects
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {missingProjects ? (
        <BuildsEmptyState
          capabilities={buildCapabilities}
          onRunBuild={() => setBuildDrawerOpen(true)}
          runtimeMode={runtimeMode}
          state="missing-projects"
        />
      ) : (
        <BuildCollection
          activeFilterCount={activeFilterCount}
          builds={builds}
          direction={direction}
          emptyState={
            <BuildsEmptyState
              capabilities={buildCapabilities}
              onRunBuild={() => setBuildDrawerOpen(true)}
              runtimeMode={runtimeMode}
              state="no-builds"
            />
          }
          error={buildsQuery.error}
          filters={
            <BuildFilters
              branches={branchSuggestions}
              filters={search}
              onChange={updateSearch}
            />
          }
          isFiltered={hasFilters}
          isLoading={buildsQuery.isLoading}
          isRefreshing={buildsQuery.isFetching && !buildsQuery.isLoading}
          onClearFilters={() =>
            updateSearch({
              q: undefined,
              project: undefined,
              pipeline: undefined,
              status: undefined,
              trigger: undefined,
              branch: undefined,
              actor: undefined,
              from: undefined,
              to: undefined,
              page: undefined,
            })
          }
          onPageChange={(nextPage) =>
            updateSearch({ page: nextPage > 1 ? nextPage : undefined })
          }
          onSortChange={handleSortChange}
          onRetry={() => void buildsQuery.refetch()}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          page={page}
          pageSize={pageSize}
          query={search.q ?? ''}
          sort={sort}
          total={total}
        />
      )}
    </PageLayout>
  )
}
