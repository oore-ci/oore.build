import { lazy, Suspense, useCallback, useState } from 'react'
import {
  Link,
  createFileRoute,
  redirect,
  useSearch,
} from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Folder02Icon,
  Link04Icon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useIntegrations } from '@/hooks/use-integrations'
import { useProjects } from '@/hooks/use-projects'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { useSetupStatus } from '@/hooks/use-setup'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { useAuthStore } from '@/stores/auth-store'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'
import { Button } from '@/components/ui/button'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import type { SortDirection } from '@/components/data-table-features'
import type { ListIntegrationsResponse, Project } from '@oore/client/models'
import { PageMeta } from '@/lib/seo'
import { ProjectCollection } from './-project-collection'
import type { ProjectSort } from './-project-collection'

const loadCreateProjectDialog = () => import('./-create-project-dialog')
const CreateProjectDialog = lazy(loadCreateProjectDialog)

interface ProjectsSearch {
  direction?: SortDirection
  openCreate?: string
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: ProjectSort
}

const PROJECT_SORT_VALUES = new Set<ProjectSort>([
  'created_at',
  'updated_at',
  'name',
])

function selectHasActiveIntegration({
  active_total,
}: ListIntegrationsResponse): boolean {
  return active_total > 0
}

function parseSearch(search: SearchInput): ProjectsSearch {
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const sort = searchChoice(search, 'sort', PROJECT_SORT_VALUES)
  const direction =
    searchString(search, 'direction') === 'asc' ? 'asc' : undefined
  const q = searchString(search, 'q')?.trim() ?? ''

  return {
    q: q || undefined,
    sort,
    direction,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
    openCreate: searchNumber(search, 'openCreate') === 1 ? '1' : undefined,
  }
}

export const Route = createFileRoute('/projects/')({
  validateSearch: parseSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
    if (useAuthStore.getState().user?.role === 'qa_viewer') {
      throw redirect({ to: '/' })
    }
  },
  component: ProjectsListPage,
})

function ProjectsListPage() {
  const scope = useFirstAppScope()
  const updateProgress = useFirstAppStore((state) => state.update)
  const search = useSearch({ from: '/projects/' })
  const navigate = Route.useNavigate()
  const page = search.page ?? 1
  const pageSize = search.pageSize ?? 20
  const sort = search.sort ?? 'updated_at'
  const direction = search.direction ?? 'desc'
  const projectsQuery = useProjects({
    search: search.q,
    sort,
    direction,
    limit: pageSize,
    offset: (page - 1) * pageSize,
  })
  const integrationsQuery = useIntegrations(
    { limit: 1 },
    {
      select: selectHasActiveIntegration,
    },
  )
  const setupStatusQuery = useSetupStatus()
  const [canWriteProjects, canWriteIntegrations] = useHasPermissions([
    'projects:write',
    'integrations:write',
  ])
  const instanceRole = useAuthStore((state) => state.user?.role)
  const canManageEveryProject =
    instanceRole === 'owner' || instanceRole === 'admin'
  const [createOpen, setCreateOpen] = useState(false)

  const projects = projectsQuery.data?.projects ?? []
  const total = projectsQuery.data?.total ?? 0
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const integrationsResolved =
    !integrationsQuery.isLoading && !integrationsQuery.error
  const noConnectedSources =
    runtimeMode === 'remote' &&
    integrationsResolved &&
    integrationsQuery.data === false

  const openCreateFromSearch =
    search.openCreate === '1' &&
    !projectsQuery.isLoading &&
    !projectsQuery.error &&
    canWriteProjects
  const isCreateOpen = createOpen || openCreateFromSearch

  function updateSearch(updates: Partial<ProjectsSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(page, pageSize, projectsQuery.data?.total, (nextPage) => {
    updateSearch({ page: nextPage === 1 ? undefined : nextPage })
  })

  function handleCreateOpenChange(open: boolean) {
    setCreateOpen(open)
    if (!open && search.openCreate === '1') {
      updateSearch({ openCreate: undefined })
    }
  }

  function handleSortChange(nextSort: ProjectSort, next: SortDirection) {
    updateSearch({ sort: nextSort, direction: next, page: undefined })
  }

  const canManageProject = useCallback(
    (project: Project) =>
      canManageEveryProject ||
      (canWriteProjects &&
        hasProjectPermission(project.current_user_role, 'projects:write')),
    [canManageEveryProject, canWriteProjects],
  )

  const hasSearch = !!search.q
  const showTrueEmpty =
    !projectsQuery.isLoading &&
    !projectsQuery.error &&
    total === 0 &&
    !hasSearch
  const showFilteredEmpty =
    !projectsQuery.isLoading && !projectsQuery.error && total === 0 && hasSearch

  return (
    <PageLayout width="wide" fill>
      <PageMeta title="Projects" noindex />
      <PageHeader
        title="Projects"
        description="Repositories, pipelines, and build access."
        actions={
          canWriteProjects ? (
            <Button onClick={() => setCreateOpen(true)}>
              <HugeiconsIcon icon={Add01Icon} />
              New project
            </Button>
          ) : undefined
        }
      />

      <ProjectCollection
        canManageProject={canManageProject}
        direction={direction}
        emptyState={
          showTrueEmpty ? (
            <Empty className="border bg-card">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={Folder02Icon} />
                </EmptyMedia>
                <EmptyTitle>Create your first project</EmptyTitle>
                <EmptyDescription>
                  {runtimeMode === 'local'
                    ? 'Choose a local Git repository to create your first project.'
                    : noConnectedSources
                      ? 'Connect a source before creating your first remote project.'
                      : 'Create a project from a connected source repository to define pipelines and start builds.'}
                </EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                {runtimeMode === 'remote' && noConnectedSources ? (
                  canWriteIntegrations ? (
                    <Button
                      render={<Link to="/settings/integrations" />}
                      onClick={() =>
                        updateProgress(scope, { projectDraft: { name: '' } })
                      }
                      nativeButton={false}
                    >
                      <HugeiconsIcon icon={Link04Icon} />
                      Connect source
                    </Button>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      Ask an owner or admin to connect a source.
                    </p>
                  )
                ) : canWriteProjects ? (
                  <Button onClick={() => setCreateOpen(true)}>
                    <HugeiconsIcon icon={Add01Icon} />
                    Create project
                  </Button>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    Ask an owner or admin to create the first project.
                  </p>
                )}
              </EmptyContent>
            </Empty>
          ) : showFilteredEmpty ? (
            <Empty className="border bg-card">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={Search01Icon} />
                </EmptyMedia>
                <EmptyTitle>No matching projects</EmptyTitle>
                <EmptyDescription>
                  Try a different search or clear the current query.
                </EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                <Button
                  variant="outline"
                  onClick={() =>
                    updateSearch({ q: undefined, page: undefined })
                  }
                >
                  Clear search
                </Button>
              </EmptyContent>
            </Empty>
          ) : null
        }
        error={projectsQuery.error}
        isLoading={projectsQuery.isLoading}
        isRefreshing={projectsQuery.isFetching && !projectsQuery.isLoading}
        onPageChange={(nextPage) =>
          updateSearch({ page: nextPage > 1 ? nextPage : undefined })
        }
        onRetry={() => void projectsQuery.refetch()}
        onSearch={(value) =>
          updateSearch({ q: value.trim() || undefined, page: undefined })
        }
        onSortChange={handleSortChange}
        page={page}
        pageSize={pageSize}
        projects={projects}
        query={search.q ?? ''}
        sort={sort}
        total={total}
      />

      {isCreateOpen ? (
        <Suspense fallback={null}>
          <CreateProjectDialog open onOpenChange={handleCreateOpenChange} />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
