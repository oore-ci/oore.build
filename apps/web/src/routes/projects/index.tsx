import { lazy, Suspense, useMemo, useState } from 'react'
import {
  Link,
  createFileRoute,
  redirect,
  useSearch,
} from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Plus as Add01Icon,
  Folder as Folder02Icon,
  Info as InformationCircleIcon,
  Link2 as Link04Icon,
  Search as Search01Icon,
} from 'lucide-react'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useIntegrations } from '@/hooks/use-integrations'
import { useProjects } from '@/hooks/use-projects'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { useSetupStatus } from '@/hooks/use-setup'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { CollectionSearchInput } from '@/components/collection-search-input'
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
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import type { SortDirection } from '@/components/collection-controls'
import { PageMeta } from '@/lib/seo'
import { ProjectInventory } from './-project-inventory'
import type { ProjectSort } from './-project-inventory'

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

const PROJECT_SORT_OPTIONS: Record<ProjectSort, string> = {
  updated_at: 'Recently updated',
  created_at: 'Recently created',
  name: 'Name',
}

const PROJECT_SORT_VALUES = new Set<ProjectSort>([
  'created_at',
  'updated_at',
  'name',
])

function parseSearch(search: Record<string, unknown>): ProjectsSearch {
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const sort = search.sort as ProjectSort
  const direction = search.direction === 'asc' ? 'asc' : undefined
  const q = typeof search.q === 'string' ? search.q.trim() : ''

  return {
    q: q || undefined,
    sort: PROJECT_SORT_VALUES.has(sort) ? sort : undefined,
    direction,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
    openCreate: search.openCreate === '1' ? '1' : undefined,
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
  const integrationsQuery = useIntegrations()
  const setupStatusQuery = useSetupStatus()
  const canWriteProjects = useHasPermission('projects', 'write')
  const instanceRole = useAuthStore((state) => state.user?.role)
  const canManageEveryProject =
    instanceRole === 'owner' || instanceRole === 'admin'
  const canWriteIntegrations = useHasPermission('integrations', 'write')
  const [createOpen, setCreateOpen] = useState(false)

  const projects = useMemo(
    () => projectsQuery.data?.projects ?? [],
    [projectsQuery.data?.projects],
  )
  const total = projectsQuery.data?.total ?? 0
  const integrations = useMemo(
    () => integrationsQuery.data?.integrations ?? [],
    [integrationsQuery.data?.integrations],
  )
  const activeIntegrationsCount = useMemo(
    () =>
      integrations.filter((integration) => integration.status === 'active')
        .length,
    [integrations],
  )
  const runtimeMode = setupStatusQuery.data?.runtime_mode ?? 'local'
  const integrationsResolved =
    !integrationsQuery.isLoading && !integrationsQuery.error
  const noConnectedSources =
    runtimeMode === 'remote' &&
    integrationsResolved &&
    activeIntegrationsCount === 0

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

  const hasSearch = !!search.q
  const showTrueEmpty =
    !projectsQuery.isLoading &&
    !projectsQuery.error &&
    total === 0 &&
    !hasSearch
  const showFilteredEmpty =
    !projectsQuery.isLoading && !projectsQuery.error && total === 0 && hasSearch

  return (
    <PageLayout width="wide">
      <PageMeta title="Projects" noindex />
      <PageHeader
        title="Projects"
        description="Repository and pipeline entry points for your build system."
        actions={
          canWriteProjects ? (
            <Button
              onMouseEnter={() => void loadCreateProjectDialog()}
              onFocus={() => void loadCreateProjectDialog()}
              onClick={() => setCreateOpen(true)}
            >
              <DynamicLucideIcon icon={Add01Icon} />
              New project
            </Button>
          ) : undefined
        }
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <CollectionSearchInput
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
          placeholder="Search projects"
          ariaLabel="Search projects"
        />
        <NativeSelect
          className="w-full sm:hidden"
          aria-label="Sort projects"
          value={sort}
          onChange={(event) =>
            handleSortChange(event.target.value as ProjectSort, direction)
          }
        >
          {Object.entries(PROJECT_SORT_OPTIONS).map(([value, label]) => (
            <NativeSelectOption key={value} value={value}>
              {label}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>

      {projectsQuery.error ? (
        <Alert variant="destructive">
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>Failed to load projects: {projectsQuery.error.message}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void projectsQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {showTrueEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <DynamicLucideIcon icon={Folder02Icon} />
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
                  nativeButton={false}
                >
                  <DynamicLucideIcon icon={Link04Icon} />
                  Connect source
                </Button>
              ) : (
                <p className="text-xs text-muted-foreground">
                  Ask an owner or admin to connect a source.
                </p>
              )
            ) : canWriteProjects ? (
              <Button
                onMouseEnter={() => void loadCreateProjectDialog()}
                onFocus={() => void loadCreateProjectDialog()}
                onClick={() => setCreateOpen(true)}
              >
                <DynamicLucideIcon icon={Add01Icon} />
                Create project
              </Button>
            ) : (
              <p className="text-xs text-muted-foreground">
                Ask an owner or admin to create the first project.
              </p>
            )}
          </EmptyContent>
        </Empty>
      ) : null}

      {showFilteredEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <DynamicLucideIcon icon={Search01Icon} />
            </EmptyMedia>
            <EmptyTitle>No matching projects</EmptyTitle>
            <EmptyDescription>
              Try a different search or clear the current query.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              variant="outline"
              onClick={() => updateSearch({ q: undefined, page: undefined })}
            >
              Clear search
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!projectsQuery.error && (projectsQuery.isLoading || total > 0) ? (
        <ProjectInventory
          canManageProject={(project) =>
            canManageEveryProject ||
            (canWriteProjects &&
              hasProjectPermission(
                project.current_user_role,
                'projects',
                'write',
              ))
          }
          direction={direction}
          isLoading={projectsQuery.isLoading}
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

      {isCreateOpen ? (
        <Suspense fallback={null}>
          <CreateProjectDialog open onOpenChange={handleCreateOpenChange} />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
