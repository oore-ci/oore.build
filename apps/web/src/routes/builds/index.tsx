import { lazy, Suspense, useMemo, useState } from 'react'
import {
  Link,
  createFileRoute,
  redirect,
  useSearch,
} from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  InformationCircleIcon,
  Link04Icon,
  PlayIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  BUILD_STATUS_FILTER_OPTIONS,
  getStatusVariant,
} from '@/lib/status-variants'
import { useAuthStore } from '@/stores/auth-store'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import SetupHint from '@/components/setup-hint'
import { Skeleton } from '@/components/ui/skeleton'
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
import { cn } from '@/lib/utils'
import type { Build, Project } from '@/lib/types'

const loadTriggerBuildDialog = () => import('@/components/trigger-build-dialog')
const TriggerBuildDialog = lazy(loadTriggerBuildDialog)

type BuildSort =
  'created_at' | 'status' | 'project_name' | 'pipeline_name' | 'branch'

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
  staticData: { breadcrumbLabel: 'Builds' },
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

function BranchSearch({
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
          const next = event.target.value
          setValue(next)
          debouncedSearch(next)
        }}
        placeholder="Search by branch"
        aria-label="Search builds by branch"
        className="pl-9"
      />
    </div>
  )
}

function projectName(build: Build, projects: Array<Project>) {
  return (
    build.context?.project_name ??
    projects.find((project) => project.id === build.project_id)?.name ??
    'Unknown project'
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
  const projectsQuery = useProjects({
    limit: 200,
    sort: 'name',
    direction: 'asc',
  })
  const setupStatusQuery = useSetupStatus()
  const canTriggerBuildGlobally = useHasPermission('builds', 'write')
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
    projects.some((project) =>
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
              <HugeiconsIcon icon={PlayIcon} />
              Run build
            </Button>
          ) : undefined
        }
      />

      {!missingProjects ? (
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
          <BranchSearch
            key={search.q ?? ''}
            initialValue={search.q ?? ''}
            onSearch={(value) =>
              updateSearch({ q: value.trim() || undefined, page: undefined })
            }
          />
          <div className="grid grid-cols-2 gap-3 sm:flex sm:flex-wrap lg:ml-auto">
            <Select
              value={search.project ?? 'all'}
              onValueChange={(value) =>
                updateSearch({
                  project: value && value !== 'all' ? value : undefined,
                  page: undefined,
                })
              }
              items={Object.fromEntries([
                ['all', 'All projects'],
                ...projects.map(
                  (project) => [project.id, project.name] as const,
                ),
              ])}
              disabled={!projectsResolved}
            >
              <SelectTrigger
                className="w-full sm:w-44"
                aria-label="Filter by project"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All projects</SelectItem>
                {projects.map((project) => (
                  <SelectItem key={project.id} value={project.id}>
                    {project.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
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
              items={BUILD_SORT_OPTIONS}
            >
              <SelectTrigger
                className="col-span-2 w-full sm:w-40 lg:hidden"
                aria-label="Sort builds"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(BUILD_SORT_OPTIONS).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {hasFilters ? (
              <Button
                variant="ghost"
                size="sm"
                className="col-span-2 sm:hidden"
                onClick={() =>
                  updateSearch({
                    q: undefined,
                    project: undefined,
                    status: undefined,
                    page: undefined,
                  })
                }
              >
                Clear filters
              </Button>
            ) : null}
            <Button
              variant="ghost"
              size="sm"
              className={cn(
                'hidden sm:inline-flex',
                !hasFilters && 'invisible',
              )}
              aria-hidden={!hasFilters}
              tabIndex={hasFilters ? undefined : -1}
              onClick={() =>
                updateSearch({
                  q: undefined,
                  project: undefined,
                  status: undefined,
                  page: undefined,
                })
              }
            >
              Clear filters
            </Button>
          </div>
        </div>
      ) : null}

      {projectsQuery.error ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
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

      {missingProjects ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Create project first
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              {runtimeMode === 'local'
                ? 'Builds run through project pipelines. Create your first project from a local Git repository.'
                : 'Builds run through project pipelines. Create your first project before triggering builds.'}
            </p>
            <SetupHint
              title="Fastest path to the first build"
              items={[
                runtimeMode === 'local'
                  ? 'Create a project from a local repository path on the runner host.'
                  : 'Connect GitHub or GitLab for repository discovery and webhook triggers.',
                'Create a pipeline and choose the platforms Oore should build.',
                'Trigger the first build from the pipeline page or a configured webhook.',
              ]}
            />
            <div className="flex flex-wrap items-center gap-2">
              {canWriteProjects ? (
                <Button render={<Link to="/projects" />} nativeButton={false}>
                  Go to projects
                  <HugeiconsIcon icon={ArrowRight01Icon} />
                </Button>
              ) : (
                <p className="text-xs text-muted-foreground">
                  Ask an owner, admin, or developer to create the first project.
                </p>
              )}
              {runtimeMode === 'remote' && canWriteIntegrations ? (
                <Button
                  variant="outline"
                  render={<Link to="/settings/integrations" />}
                  nativeButton={false}
                >
                  <HugeiconsIcon icon={Link04Icon} />
                  Connect source
                </Button>
              ) : null}
            </div>
          </CardContent>
        </Card>
      ) : null}

      {showTrueEmpty ? (
        <Empty className="bg-card">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HugeiconsIcon icon={PlayIcon} />
            </EmptyMedia>
            <EmptyTitle>No builds yet</EmptyTitle>
            <EmptyDescription>
              Run a pipeline to see its status, output, and artifacts here.
            </EmptyDescription>
          </EmptyHeader>
          {canTriggerBuild ? (
            <EmptyContent>
              <Button
                onMouseEnter={() => void loadTriggerBuildDialog()}
                onFocus={() => void loadTriggerBuildDialog()}
                onClick={() => setTriggerBuildOpen(true)}
              >
                <HugeiconsIcon icon={PlayIcon} />
                Run first build
              </Button>
            </EmptyContent>
          ) : null}
        </Empty>
      ) : null}

      {showFilteredEmpty ? (
        <Empty className="bg-card">
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
            <Button
              variant="outline"
              onClick={() =>
                updateSearch({
                  q: undefined,
                  project: undefined,
                  status: undefined,
                  page: undefined,
                })
              }
            >
              Clear filters
            </Button>
          </EmptyContent>
        </Empty>
      ) : null}

      {!missingProjects &&
      !buildsQuery.error &&
      (buildsQuery.isLoading || total > 0) ? (
        <section aria-label="Build queue and history" className="min-w-0">
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
                    <p className="truncate text-sm">
                      {projectName(build, projects)}
                      {build.context?.pipeline_name ? (
                        <span className="text-muted-foreground">
                          {' · '}
                          {build.context.pipeline_name}
                        </span>
                      ) : null}
                    </p>
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
                    sortKey="project_name"
                    direction={direction}
                    onSortChange={handleSortChange}
                  >
                    Project
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
                  <TableHead className="hidden lg:table-cell">Commit</TableHead>
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
                          <Skeleton className="h-8 w-36" />
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
                            {projectName(build, projects)}
                          </p>
                          {build.context?.pipeline_name ? (
                            <p className="text-xs text-muted-foreground">
                              {build.context.pipeline_name}
                            </p>
                          ) : null}
                        </TableCell>
                        <TableCell>
                          <Badge variant={getStatusVariant(build.status)}>
                            {build.status}
                          </Badge>
                        </TableCell>
                        <TableCell className="hidden lg:table-cell">
                          <Badge variant="outline">{build.trigger_type}</Badge>
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
