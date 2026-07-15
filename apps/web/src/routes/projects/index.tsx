import { lazy, Suspense, useMemo, useState } from 'react'
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
  InformationCircleIcon,
  Link04Icon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useIntegrations } from '@/hooks/use-integrations'
import { useProjects } from '@/hooks/use-projects'
import { useHasPermission } from '@/hooks/use-permissions'
import { useSetupStatus } from '@/hooks/use-setup'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  CollectionPagination,
  SortableTableHead,
} from '@/components/collection-controls'
import type { SortDirection } from '@/components/collection-controls'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
import RepositoryAvatar from '@/components/repository-avatar'
import type { Project } from '@/lib/types'

const loadCreateProjectDialog = () => import('./-create-project-dialog')
const CreateProjectDialog = lazy(loadCreateProjectDialog)

type ProjectSort = 'created_at' | 'updated_at' | 'name'

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
  staticData: { breadcrumbLabel: 'Projects' },
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

function ProjectSearch({
  initialValue,
  onSearch,
}: {
  initialValue: string
  onSearch: (value: string) => void
}) {
  const [value, setValue] = useState(initialValue)
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
          const nextValue = event.target.value
          setValue(nextValue)
          debouncedSearch(nextValue)
        }}
        placeholder="Search projects"
        aria-label="Search projects"
        className="pl-9"
      />
    </div>
  )
}

function ProjectIdentity({ project }: { project: Project }) {
  return (
    <Link
      to="/projects/$projectId"
      params={{ projectId: project.id }}
      className="group flex min-w-0 items-center gap-3 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      {project.repository_full_name ? (
        <RepositoryAvatar
          fullName={project.repository_full_name}
          avatarUrl={project.repository_avatar_url}
          repositoryId={project.repository_id}
          provider={project.repository_provider}
        />
      ) : null}
      <span className="min-w-0">
        <span className="block truncate font-medium group-hover:underline">
          {project.name}
        </span>
        <span className="block truncate font-mono text-[11px] text-muted-foreground">
          {project.repository_full_name ?? project.id.slice(0, 8)}
        </span>
      </span>
    </Link>
  )
}

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
              <HugeiconsIcon icon={Add01Icon} />
              New project
            </Button>
          ) : undefined
        }
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <ProjectSearch
          key={search.q ?? ''}
          initialValue={search.q ?? ''}
          onSearch={(value) =>
            updateSearch({ q: value.trim() || undefined, page: undefined })
          }
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
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
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
              <Button
                onMouseEnter={() => void loadCreateProjectDialog()}
                onFocus={() => void loadCreateProjectDialog()}
                onClick={() => setCreateOpen(true)}
              >
                <HugeiconsIcon icon={Add01Icon} />
                Create project
              </Button>
            ) : (
              <p className="text-xs text-muted-foreground">
                Ask an owner, admin, or developer to create the first project.
              </p>
            )}
          </EmptyContent>
        </Empty>
      ) : null}

      {showFilteredEmpty ? (
        <Empty className="bg-card">
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
              onClick={() => updateSearch({ q: undefined, page: undefined })}
            >
              Clear search
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!projectsQuery.error && (projectsQuery.isLoading || total > 0) ? (
        <section aria-label="Project inventory" className="min-w-0">
          <div className="divide-y sm:hidden">
            {projectsQuery.isLoading
              ? Array.from({ length: 4 }, (_, index) => (
                  <div key={index} className="space-y-2 py-4">
                    <Skeleton className="h-5 w-2/3" />
                    <Skeleton className="h-4 w-1/2" />
                  </div>
                ))
              : projects.map((project) => (
                  <div key={project.id} className="space-y-2 py-4">
                    <ProjectIdentity project={project} />
                    <div className="flex items-center justify-between gap-4 pl-11 text-xs text-muted-foreground">
                      <span className="truncate font-mono">
                        {project.default_branch ?? 'Branch not set'}
                      </span>
                      <span className="shrink-0">
                        {relativeTime(project.updated_at)}
                      </span>
                    </div>
                  </div>
                ))}
          </div>

          <div className="hidden sm:block">
            <Table>
              <TableHeader>
                <TableRow>
                  <SortableTableHead
                    sort={sort}
                    sortKey="name"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Project
                  </SortableTableHead>
                  <TableHead>Default branch</TableHead>
                  <TableHead className="hidden lg:table-cell">
                    Description
                  </TableHead>
                  <SortableTableHead
                    sort={sort}
                    sortKey="updated_at"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Updated
                  </SortableTableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {projectsQuery.isLoading
                  ? Array.from({ length: 5 }, (_, index) => (
                      <TableRow key={index}>
                        <TableCell>
                          <Skeleton className="h-8 w-48" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-4 w-24" />
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Skeleton className="h-4 w-40" />
                        </TableCell>
                        <TableCell>
                          <Skeleton className="h-4 w-20" />
                        </TableCell>
                      </TableRow>
                    ))
                  : projects.map((project) => (
                      <TableRow key={project.id}>
                        <TableCell>
                          <ProjectIdentity project={project} />
                        </TableCell>
                        <TableCell className="font-mono text-xs text-muted-foreground">
                          {project.default_branch ?? 'not set'}
                        </TableCell>
                        <TableCell className="hidden max-w-[30ch] truncate text-sm text-muted-foreground lg:table-cell">
                          {project.description ?? 'No description'}
                        </TableCell>
                        <TableCell className="text-sm text-muted-foreground">
                          {relativeTime(project.updated_at)}
                        </TableCell>
                      </TableRow>
                    ))}
              </TableBody>
            </Table>
          </div>

          {!projectsQuery.isLoading ? (
            <CollectionPagination
              page={page}
              pageSize={pageSize}
              total={total}
              onPageChange={(nextPage) =>
                updateSearch({ page: nextPage > 1 ? nextPage : undefined })
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

      {isCreateOpen ? (
        <Suspense fallback={null}>
          <CreateProjectDialog open onOpenChange={handleCreateOpenChange} />
        </Suspense>
      ) : null}
    </PageLayout>
  )
}
