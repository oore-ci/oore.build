import { lazy, Suspense, useMemo, useState } from 'react'
import { createFileRoute, redirect, useSearch } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Info as InformationCircleIcon, Play as PlayIcon } from 'lucide-react'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { useAllProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import type { SortDirection } from '@/components/collection-controls'
import { PageMeta } from '@/lib/seo'
import { BuildInventory } from './-build-inventory'
import type { BuildSort } from './-build-inventory'
import { BuildsEmptyState } from './-builds-empty-state'
import { BuildFilters } from './-build-filters'

const loadTriggerBuildDialog = () => import('@/components/trigger-build-dialog')
const TriggerBuildDialog = lazy(loadTriggerBuildDialog)

interface BuildsSearch {
  direction?: SortDirection
  page?: number
  pageSize?: 20 | 50 | 100
  project?: string
  q?: string
  sort?: BuildSort
  status?: string
}

const BUILD_SORT_OPTIONS: Record<BuildSort, string> = {
  created_at: 'Newest first',
  status: 'Status',
  project_name: 'Project',
  pipeline_name: 'Pipeline',
  branch: 'Branch',
}

const BUILD_SORT_VALUES = new Set<BuildSort>(
  Object.keys(BUILD_SORT_OPTIONS) as Array<BuildSort>,
)

function parseSearch(search: Record<string, unknown>): BuildsSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const q = typeof search.q === 'string' ? search.q.trim() : ''
  const project =
    typeof search.project === 'string' ? search.project.trim() : ''
  const status =
    typeof search.status === 'string' &&
    search.status in BUILD_STATUS_FILTER_OPTIONS
      ? search.status
      : ''
  const sort = search.sort as BuildSort

  return {
    q: q || undefined,
    project: project || undefined,
    status: status && status !== 'all' ? status : undefined,
    sort: BUILD_SORT_VALUES.has(sort) ? sort : undefined,
    direction: search.direction === 'asc' ? 'asc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
  }
}

export const Route = createFileRoute('/builds/')({
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
    if (useAuthStore.getState().user?.role === 'qa_viewer') {
      throw redirect({ to: '/' })
    }
  },
  component: OperationsBuildsPage,
})

function OperationsBuildsPage() {
  const search = useSearch({ from: '/builds/' })
  const navigate = Route.useNavigate()
  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'created_at'
  const direction = search.direction ?? 'desc'
  const buildsQuery = useBuilds({
    branch: search.q,
    project_id: search.project,
    status: search.status,
    sort,
    direction,
    limit: pageSize,
    offset: (page - 1) * pageSize,
  })
  const projectsQuery = useAllProjects({
    sort: 'name',
    direction: 'asc',
  })
  const setupStatusQuery = useSetupStatus()
  const canTriggerBuildGlobally = useHasPermission('builds', 'write')
  const instanceRole = useAuthStore((state) => state.user?.role)
  const canTriggerEveryProject =
    instanceRole === 'owner' || instanceRole === 'admin'
  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteIntegrations = useHasPermission('integrations', 'write')
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)

  const builds = useMemo(
    () => buildsQuery.data?.builds ?? [],
    [buildsQuery.data?.builds],
  )
  const projects = useMemo(
    () => projectsQuery.data?.projects ?? [],
    [projectsQuery.data?.projects],
  )
  const total = buildsQuery.data?.total ?? 0
  const canTriggerBuild =
    canTriggerBuildGlobally &&
    projects.some(
      (project) =>
        canTriggerEveryProject ||
        hasProjectPermission(project.current_user_role, 'builds', 'write'),
    )
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const projectsResolved = !projectsQuery.isLoading && !projectsQuery.error
  const missingProjects = projectsResolved && projects.length === 0
  const hasFilters = !!search.q || !!search.project || !!search.status

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

  const showFilteredEmpty =
    !buildsQuery.isLoading && !buildsQuery.error && total === 0 && hasFilters
  const showTrueEmpty =
    !buildsQuery.isLoading &&
    !buildsQuery.error &&
    total === 0 &&
    !hasFilters &&
    !missingProjects

  return (
    <PageLayout width="wide">
      <PageMeta title="Builds" noindex />
      <PageHeader
        title="Builds"
        description="Queue, execution, and historical run inventory across projects."
        actions={
          !missingProjects && canTriggerBuild ? (
            <Button
              onMouseEnter={() => void loadTriggerBuildDialog()}
              onFocus={() => void loadTriggerBuildDialog()}
              onClick={() => setTriggerBuildOpen(true)}
            >
              <DynamicLucideIcon icon={PlayIcon} />
              Run build
            </Button>
          ) : undefined
        }
      />

      {!missingProjects ? (
        <BuildFilters
          direction={direction}
          filters={search}
          onChange={updateSearch}
          onSortChange={handleSortChange}
          projects={projects}
          projectsResolved={projectsResolved}
          sort={sort}
        />
      ) : null}

      {projectsQuery.error ? (
        <Alert>
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
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

      {buildsQuery.error ? (
        <Alert variant="destructive">
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
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

      <BuildsEmptyState
        capabilities={{
          triggerBuild: canTriggerBuild,
          writeIntegrations: canWriteIntegrations,
          writeProjects: canWriteProjects,
        }}
        onClearFilters={() =>
          updateSearch({
            q: undefined,
            project: undefined,
            status: undefined,
            page: undefined,
          })
        }
        onRunBuild={() => setTriggerBuildOpen(true)}
        onWarmBuildDialog={() => void loadTriggerBuildDialog()}
        runtimeMode={runtimeMode}
        state={
          missingProjects
            ? 'missing-projects'
            : showTrueEmpty
              ? 'no-builds'
              : showFilteredEmpty
                ? 'no-results'
                : null
        }
      />

      {!missingProjects &&
      !buildsQuery.error &&
      (buildsQuery.isLoading || total > 0) ? (
        <BuildInventory
          builds={builds}
          direction={direction}
          isLoading={buildsQuery.isLoading}
          onPageChange={(nextPage) =>
            updateSearch({ page: nextPage > 1 ? nextPage : undefined })
          }
          onPageSizeChange={(nextPageSize) =>
            updateSearch({
              pageSize:
                nextPageSize === 20 ? undefined : (nextPageSize as 50 | 100),
              page: undefined,
            })
          }
          onSortChange={handleSortChange}
          page={page}
          pageSize={pageSize}
          projects={projects}
          sort={sort}
          total={total}
        />
      ) : null}

      {triggerBuildOpen ? (
        <Suspense fallback={null}>
          <TriggerBuildDialog
            open
            onOpenChange={setTriggerBuildOpen}
            description="Choose a project and pipeline to run a manual build."
            onBuildCreated={(buildId) => {
              void navigate({
                to: '/builds/$buildId',
                params: { buildId },
              })
            }}
          />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
