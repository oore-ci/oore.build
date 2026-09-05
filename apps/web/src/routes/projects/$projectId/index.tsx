import { lazy, Suspense, useState } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
  Delete02Icon,
  InformationCircleIcon,
  PlayIcon,
} from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import { searchChoice, searchNumber, searchString } from '@/lib/search-input'
import type { SearchInput } from '@/lib/search-input'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { usePageClamp } from '@/hooks/use-page-clamp'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import { usePipelines, useRepositoryWorkflows } from '@/hooks/use-pipelines'
import { useDeleteProject, useProject } from '@/hooks/use-projects'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { relativeTime } from '@/lib/format-utils'
import { isOoreApiError } from '@oore/client/client'
import { PageMeta } from '@/lib/seo'
import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import type { ListBuildsResponse } from '@oore/client/models'
import type { SortDirection } from '@/components/data-table-features'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import RepositoryAvatar from '@/components/repository-avatar'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ProjectPipelinesTab } from './-project-pipelines-tab'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'

const ProjectOverview = lazy(() => import('./-project-overview'))

const FirstAppProgress = lazy(() => import('@/components/first-app-progress'))

import type { ProjectBuildSort } from './-project-build-sort'

const loadTriggerBuildDrawer = () => import('@/components/trigger-build-drawer')
const TriggerBuildDrawer = lazy(loadTriggerBuildDrawer)
const ProjectBuildsTab = lazy(() =>
  import('./-project-detail-tabs').then((module) => ({
    default: module.ProjectBuildsTab,
  })),
)
const loadProjectSettingsForm = () => import('./-project-settings-form')
const ProjectSettingsForm = lazy(() =>
  loadProjectSettingsForm().then((module) => ({
    default: module.ProjectSettingsForm,
  })),
)
const loadProjectAccessCard = () => import('./-project-access-card')
const ProjectAccessCard = lazy(() =>
  loadProjectAccessCard().then((module) => ({
    default: module.ProjectAccessCard,
  })),
)

const TAB_VALUES = ['overview', 'pipelines', 'builds', 'settings'] as const
type TabValue = (typeof TAB_VALUES)[number]

interface ProjectDetailSearch {
  run?: string
  runPipeline?: string
  direction?: SortDirection
  page?: number
  pageSize?: 20 | 50 | 100
  q?: string
  sort?: ProjectBuildSort
  status?: string
  tab?: TabValue
  pipelineDirection?: SortDirection
  pipelinePage?: number
  pipelinePageSize?: 20 | 50 | 100
  pipelineQ?: string
  pipelineSort?: 'created_at' | 'name'
}

const PROJECT_BUILD_SORT_VALUES = new Set<ProjectBuildSort>([
  'created_at',
  'status',
  'pipeline_name',
  'branch',
])

const EMPTY_LAST_BUILD_BY_PIPELINE = new Map<
  string,
  { status: string; time: number }
>()

function selectProjectBuildSummary({ builds, total }: ListBuildsResponse) {
  const lastBuildByPipeline = new Map<
    string,
    { status: string; time: number }
  >()

  for (const build of builds) {
    if (build.pipeline_id && !lastBuildByPipeline.has(build.pipeline_id)) {
      lastBuildByPipeline.set(build.pipeline_id, {
        status: build.status,
        time: build.queued_at,
      })
    }
  }

  return { buildCount: total, lastBuildByPipeline, builds }
}

function validateProjectSearch(search: SearchInput): ProjectDetailSearch {
  const tab = searchString(search, 'tab')
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const statusValue = searchString(search, 'status')
  const status =
    statusValue && statusValue in BUILD_STATUS_FILTER_OPTIONS ? statusValue : ''
  const sort = searchChoice(search, 'sort', PROJECT_BUILD_SORT_VALUES)
  const pipelinePage = searchNumber(search, 'pipelinePage')
  const pipelinePageSize = searchNumber(search, 'pipelinePageSize')
  const pipelineQ = searchString(search, 'pipelineQ')?.trim() ?? ''
  const selectedTab = TAB_VALUES.find((value) => value === tab)

  return {
    tab: selectedTab,
    run: searchNumber(search, 'run') === 1 ? '1' : undefined,
    runPipeline: searchString(search, 'runPipeline'),
    q: q || undefined,
    status: status && status !== 'all' ? status : undefined,
    sort,
    direction: searchString(search, 'direction') === 'asc' ? 'asc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
    pipelineQ: pipelineQ || undefined,
    pipelineSort:
      searchString(search, 'pipelineSort') === 'name' ? 'name' : undefined,
    pipelineDirection:
      searchString(search, 'pipelineDirection') === 'asc' ? 'asc' : undefined,
    pipelinePage:
      Number.isInteger(pipelinePage) && pipelinePage > 1
        ? pipelinePage
        : undefined,
    pipelinePageSize:
      pipelinePageSize === 50 || pipelinePageSize === 100
        ? pipelinePageSize
        : undefined,
  }
}

export const Route = createFileRoute('/projects/$projectId/')({
  validateSearch: validateProjectSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: ProjectDetailPage,
})

function ProjectDetailPage() {
  const { projectId } = Route.useParams()
  const scope = useFirstAppScope()
  const guidedProjectId = useFirstAppStore(
    (state) => state.progress[scope]?.projectId,
  )
  const search = Route.useSearch()
  const { tab } = search
  const navigate = Route.useNavigate()
  const projectQuery = useProject(projectId)
  const { data, isLoading, error } = projectQuery
  const pipelinePage = search.pipelinePage ?? 1
  const pipelinePageSize = search.pipelinePageSize ?? 20
  const pipelineSort = search.pipelineSort ?? 'created_at'
  const pipelineDirection = search.pipelineDirection ?? 'desc'
  const pipelinesQuery = usePipelines(projectId, {
    search: search.pipelineQ,
    sort: pipelineSort,
    direction: pipelineDirection,
    limit: pipelinePageSize,
    offset: (pipelinePage - 1) * pipelinePageSize,
  })
  const { data: pipelinesData } = pipelinesQuery
  const buildSummaryQuery = useBuilds(
    { project_id: projectId, limit: 20 },
    {
      refetchInterval: 15_000,
      select: selectProjectBuildSummary,
    },
  )
  const buildSummary = buildSummaryQuery.data
  const deleteMutation = useDeleteProject()
  const [
    canWritePipelinesGlobally,
    canTriggerBuildGlobally,
    canWriteInstanceSettings,
    canReadInstanceSettings,
  ] = useHasPermissions([
    'pipelines:write',
    'builds:write',
    'instance_settings:write',
    'instance_settings:read',
  ])
  const projectRole = data?.current_user_role ?? data?.project.current_user_role
  const canWriteProjects = hasProjectPermission(projectRole, 'projects:write')
  const canDeleteProjects = hasProjectPermission(projectRole, 'projects:delete')
  const canWritePipelines =
    canWritePipelinesGlobally &&
    hasProjectPermission(projectRole, 'pipelines:write')
  const canTriggerBuild =
    canTriggerBuildGlobally && hasProjectPermission(projectRole, 'builds:write')
  const canManageAccess = projectRole === 'maintainer'
  const pipelineCount = search.pipelineQ
    ? (data?.pipeline_count ?? 0)
    : (pipelinesData?.total ?? data?.pipeline_count ?? 0)
  const projectSourceAvailable = Boolean(
    data?.project.repository_id && data.project.repository_full_name,
  )
  const shouldDiscoverWorkflows =
    canWritePipelines && projectSourceAvailable && pipelineCount === 0
  const repositoryWorkflowsQuery = useRepositoryWorkflows(
    projectId,
    undefined,
    { enabled: shouldDiscoverWorkflows },
  )
  const preferencesQuery = useInstancePreferences({
    enabled: projectSourceAvailable && canReadInstanceSettings,
  })

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [dangerOpen, setDangerOpen] = useState(false)
  const [buildDrawerPipelineId, setBuildDrawerPipelineId] = useState<
    string | null
  >()

  const lastBuildByPipeline =
    buildSummary?.lastBuildByPipeline ?? EMPTY_LAST_BUILD_BY_PIPELINE
  const buildCount = buildSummary?.buildCount ?? data?.build_count ?? 0

  function updatePipelineSearch(updates: Partial<ProjectDetailSearch>) {
    void navigate({
      search: (previous) => ({ ...previous, ...updates }),
      replace: true,
    })
  }

  usePageClamp(
    pipelinePage,
    pipelinePageSize,
    pipelinesData?.total,
    (nextPage) =>
      updatePipelineSearch({
        pipelinePage: nextPage === 1 ? undefined : nextPage,
      }),
  )

  const activeTab: TabValue = tab ?? 'overview'

  const label = data?.project.name ?? 'Project Details'

  if (isLoading) {
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-10 w-72" />
        <Skeleton className="h-56 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    const notFound = isOoreApiError(error) && error.status === 404

    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              {notFound
                ? 'This project was not found or is no longer available.'
                : `Failed to load project: ${error.message}`}
            </span>
            <span className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                render={<Link to="/projects" />}
              >
                Back to projects
              </Button>
              {!notFound ? (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void projectQuery.refetch()}
                >
                  Retry
                </Button>
              ) : null}
            </span>
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { project } = data
  const pipelines = pipelinesData?.pipelines ?? []
  const pipelineQuery = search.pipelineQ ?? ''
  const projectHasSource = projectSourceAvailable
  const runnerPolicyBlockReason =
    project.repository_id && !projectHasSource
      ? ('repository_unavailable' as const)
      : canReadInstanceSettings &&
          preferencesQuery.data?.direct_macos_runner_paused
        ? ('instance_paused' as const)
        : undefined

  function setTab(value: TabValue) {
    void navigate({
      to: '/projects/$projectId',
      params: { projectId },
      search: (previous) => ({
        ...previous,
        tab: value === 'overview' ? undefined : value,
      }),
      replace: true,
    })
  }

  function handleDelete() {
    deleteMutation.mutate(projectId, {
      onSuccess: () => {
        toast.success('Project deleted')
        void navigate({ to: '/projects' })
      },
      onError: (err) => {
        toast.error(`Failed to delete project: ${err.message}`)
      },
    })
  }

  function openTriggerBuild(pipelineId?: string) {
    setBuildDrawerPipelineId(pipelineId ?? null)
  }

  const DangerIcon = dangerOpen ? ArrowDown01Icon : ArrowRight01Icon

  return (
    <PageLayout width="wide" fill={activeTab === 'builds'}>
      <PageMeta title={label} noindex />
      <PageHeader
        title={project.name}
        description={project.description ?? undefined}
        meta={
          <>
            {project.default_branch ? (
              <Badge variant="outline" className="font-mono text-[11px]">
                {project.default_branch}
              </Badge>
            ) : null}
            {project.repository_full_name ? (
              <span className="flex items-center gap-1.5">
                <RepositoryAvatar
                  fullName={project.repository_full_name}
                  avatarUrl={project.repository_avatar_url}
                  repositoryId={project.repository_id}
                  provider={project.repository_provider}
                />
                {project.repository_full_name}
              </span>
            ) : null}
            <span>Updated {relativeTime(project.updated_at)}</span>
          </>
        }
        actions={
          canTriggerBuild ? (
            <span
              title={
                pipelineCount === 0
                  ? 'Add a pipeline first before running builds'
                  : !projectHasSource
                    ? 'Connect a source repository first'
                    : undefined
              }
            >
              <Suspense
                fallback={
                  <Button disabled>
                    <HugeiconsIcon icon={PlayIcon} />
                    Run build
                  </Button>
                }
              >
                <TriggerBuildDrawer
                  fixedProjectId={projectId}
                  defaultPipelineId={
                    search.runPipeline ??
                    buildDrawerPipelineId ??
                    pipelines[0]?.id
                  }
                  open={
                    search.run === '1' || buildDrawerPipelineId !== undefined
                  }
                  onOpenChange={(open) => {
                    if (!open && search.run)
                      void navigate({
                        search: (previous) => ({
                          ...previous,
                          run: undefined,
                          runPipeline: undefined,
                        }),
                        replace: true,
                      })
                    setBuildDrawerPipelineId(
                      open ? (buildDrawerPipelineId ?? null) : undefined,
                    )
                  }}
                  defaultBranch={project.default_branch ?? undefined}
                  description="Run this project's pipeline now."
                  onBuildCreated={(buildId) => {
                    void navigate({
                      to: '/builds/$buildId',
                      params: { buildId },
                    })
                  }}
                >
                  <Button disabled={pipelineCount === 0 || !projectHasSource}>
                    <HugeiconsIcon icon={PlayIcon} />
                    Run build
                  </Button>
                </TriggerBuildDrawer>
              </Suspense>
            </span>
          ) : undefined
        }
      />
      {!project.repository_id ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              This project has no linked source repository.{' '}
              {canWriteInstanceSettings
                ? 'Choose a source before triggering builds.'
                : 'Ask an owner or admin to choose one before triggering builds.'}
            </span>
            {canWriteInstanceSettings ? (
              <Button
                size="sm"
                variant="outline"
                onClick={() => setTab('settings')}
              >
                Choose source
              </Button>
            ) : null}
          </AlertDescription>
        </Alert>
      ) : null}
      {runnerPolicyBlockReason ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            {runnerPolicyBlockReason === 'instance_paused' ? (
              canWriteInstanceSettings ? (
                <>
                  Direct macOS runner is paused. Builds can be queued, but they
                  will not start until you resume it in{' '}
                  <Link
                    to="/settings/preferences"
                    className="font-medium underline underline-offset-4"
                  >
                    Preferences
                  </Link>
                  .
                </>
              ) : (
                <>
                  Direct macOS runner is paused. Builds can be queued, but they
                  will not start. Ask an owner or admin to resume it.
                </>
              )
            ) : (
              <span className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <span>
                  Oore cannot find this project&apos;s source repository. Builds
                  remain queued until{' '}
                  {canWriteInstanceSettings
                    ? 'you repair the source link.'
                    : 'an owner or admin repairs the source link.'}
                </span>
                {canWriteInstanceSettings ? (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => setTab('settings')}
                  >
                    Repair source
                  </Button>
                ) : null}
              </span>
            )}
          </AlertDescription>
        </Alert>
      ) : null}

      {(activeTab === 'overview' || activeTab === 'pipelines') &&
      (buildCount === 0 || guidedProjectId === projectId) ? (
        <Suspense fallback={<Skeleton className="h-40 w-full" />}>
          <FirstAppProgress projectId={projectId} />
        </Suspense>
      ) : null}

      <Tabs
        value={activeTab}
        onValueChange={(value) => {
          const tabValue = TAB_VALUES.find((candidate) => candidate === value)
          if (tabValue) setTab(tabValue)
        }}
        className={activeTab === 'builds' ? 'min-h-0 flex-1' : undefined}
      >
        <TabsList variant="line">
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="pipelines">
            Pipelines
            {pipelineCount > 0 ? ` (${pipelineCount})` : ''}
          </TabsTrigger>
          <TabsTrigger value="builds">
            Builds{buildCount > 0 ? ` (${buildCount})` : ''}
          </TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>

        <TabsContent value="overview">
          {activeTab === 'overview' ? (
            <Suspense fallback={<Skeleton className="h-64 w-full" />}>
              <ProjectOverview
                projectId={projectId}
                projectName={project.name}
                builds={buildSummary?.builds ?? []}
                loading={buildSummaryQuery.isLoading}
                error={buildSummaryQuery.error}
                onRetry={() => void buildSummaryQuery.refetch()}
              />
            </Suspense>
          ) : null}
        </TabsContent>

        <ProjectPipelinesTab
          canTriggerBuild={canTriggerBuild}
          canWritePipelines={canWritePipelines}
          defaultBranch={project.default_branch ?? undefined}
          hasValidRepositoryWorkflow={
            repositoryWorkflowsQuery.data?.workflows.some(
              (workflow) => workflow.valid,
            ) ?? false
          }
          lastBuildByPipeline={lastBuildByPipeline}
          onTriggerBuild={openTriggerBuild}
          pipelines={pipelines}
          direction={pipelineDirection}
          error={pipelinesQuery.error?.message}
          isLoading={pipelinesQuery.isLoading}
          onPageChange={(page) =>
            updatePipelineSearch({
              pipelinePage: page === 1 ? undefined : page,
            })
          }
          onQueryChange={(query) =>
            updatePipelineSearch({
              pipelineQ: query.trim() || undefined,
              pipelinePage: undefined,
            })
          }
          onRetry={() => void pipelinesQuery.refetch()}
          onSortChange={(sort, direction) =>
            updatePipelineSearch({
              pipelineSort: sort === 'created_at' ? undefined : sort,
              pipelineDirection: direction === 'desc' ? undefined : direction,
              pipelinePage: undefined,
            })
          }
          page={pipelinePage}
          pageSize={pipelinePageSize}
          projectHasSource={projectHasSource}
          projectId={projectId}
          query={pipelineQuery}
          sort={pipelineSort}
          total={pipelinesQuery.data?.total ?? 0}
          workflowDiscoveryFailed={!!repositoryWorkflowsQuery.error}
          workflowDiscoveryLoading={repositoryWorkflowsQuery.isLoading}
        />

        {activeTab === 'builds' ? (
          <Suspense fallback={<Skeleton className="h-48 w-full" />}>
            <ProjectBuildsTab
              canTriggerBuild={canTriggerBuild}
              onTriggerBuild={() => openTriggerBuild()}
              pipelineCount={pipelineCount}
              projectHasSource={projectHasSource}
              projectId={projectId}
            />
          </Suspense>
        ) : null}

        {/* ---- Settings tab ---- */}
        <TabsContent value="settings">
          <div className="space-y-4 pt-2">
            {activeTab === 'settings' ? (
              <Suspense fallback={<Skeleton className="h-48 w-full" />}>
                {canManageAccess ? (
                  <ProjectAccessCard projectId={projectId} />
                ) : null}
                {canWriteProjects ? (
                  <ProjectSettingsForm
                    canChangeSource={canWriteInstanceSettings}
                    projectId={projectId}
                    currentValues={{
                      name: project.name,
                      description: project.description ?? undefined,
                      default_branch: project.default_branch ?? undefined,
                      repository_id: project.repository_id ?? undefined,
                    }}
                  />
                ) : (
                  <Card size="sm">
                    <CardContent className="text-sm text-muted-foreground">
                      You do not have permission to edit this project.
                    </CardContent>
                  </Card>
                )}
              </Suspense>
            ) : null}

            {canDeleteProjects ? (
              <Collapsible open={dangerOpen} onOpenChange={setDangerOpen}>
                <Card size="sm" className="ring-destructive/40">
                  <CardContent>
                    <CollapsibleTrigger className="flex w-full items-center justify-between text-sm font-medium text-destructive">
                      Danger zone
                      <HugeiconsIcon
                        icon={DangerIcon}
                        className="size-4 text-muted-foreground"
                        aria-hidden
                      />
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                      <div className="space-y-3 pt-4">
                        <p className="text-sm text-muted-foreground">
                          Permanently delete "{project.name}" and all associated
                          pipelines and builds. This cannot be undone.
                        </p>
                        <Button
                          variant="destructive"
                          onClick={() => setDeleteOpen(true)}
                        >
                          <HugeiconsIcon icon={Delete02Icon} />
                          Delete project
                        </Button>
                      </div>
                    </CollapsibleContent>
                  </CardContent>
                </Card>
              </Collapsible>
            ) : null}
          </div>
        </TabsContent>
      </Tabs>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete project?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{project.name}" and all associated
              pipelines and builds. This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete}>
              {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  )
}
