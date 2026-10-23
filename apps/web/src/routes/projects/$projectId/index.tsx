import { lazy, Suspense, useState } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
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
import type { SortDirection } from '@/components/data-table-features'
import { Alert, AlertDescription } from '@/components/ui/alert'
import ConfirmDialog from '@/components/ConfirmDialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DangerZone } from '@/components/danger-zone'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { NotFoundState } from '@/components/root-route-boundaries'
import { ResumeRunnersButton } from '@/components/settings/direct-runner-policy-panel'
import RepositoryAvatar from '@/components/repository-avatar'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ProjectPipelinesTab } from './-project-pipelines-tab'
import {
  EMPTY_LAST_BUILD_BY_PIPELINE,
  selectProjectBuildSummary,
} from './-project-build-summary'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { useListNavigation } from '@/hooks/use-list-navigation'

const ProjectOverview = lazy(() => import('./-project-overview'))

const SetupChecklist = lazy(() => import('@/components/setup-checklist'))

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
const ProjectSecretsPanel = lazy(() =>
  import('@/components/secrets/secrets-panel').then((module) => ({
    default: module.SecretsPanel,
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
  pipeline?: string
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

function validateProjectSearch(search: SearchInput): ProjectDetailSearch {
  const tab = searchString(search, 'tab')
  const page = searchNumber(search, 'page')
  const pageSize = searchNumber(search, 'pageSize')
  const q = searchString(search, 'q')?.trim() ?? ''
  const pipeline = searchString(search, 'pipeline')?.trim() ?? ''
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
    pipeline: pipeline || undefined,
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
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(
      instance.id,
      ['owner', 'admin', 'developer'],
      context,
    )
  },
  component: ProjectDetailPage,
})

function ProjectDetailPage() {
  useListNavigation()
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
    if (isOoreApiError(error) && error.status === 404) {
      return (
        <PageLayout width="wide">
          <PageMeta title="Project not found" noindex />
          <NotFoundState
            title="Project not found"
            description="This project was deleted, expired, or the link is wrong."
            backTo="/projects"
            backLabel="Back to projects"
          />
        </PageLayout>
      )
    }

    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>Failed to load project: {error.message}</span>
            <span className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                render={<Link to="/projects" />}
                nativeButton={false}
              >
                Back to projects
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void projectQuery.refetch()}
              >
                Retry
              </Button>
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

  // The checklist lives at the top of Overview while the project is being set
  // up. It never sits above the tabs, so switching tabs moves nothing.
  const showSetup = buildCount === 0 || guidedProjectId === projectId
  const canRunBuild = canTriggerBuild && projectHasSource && pipelineCount > 0

  // One primary action, chosen by where the project is: connect a repository,
  // add the first pipeline, or run a build. Never a disabled button with a
  // tooltip explaining why.
  const headerAction = canRunBuild ? (
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
          search.runPipeline ?? buildDrawerPipelineId ?? pipelines[0]?.id
        }
        open={search.run === '1' || buildDrawerPipelineId !== undefined}
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
        description="Choose a pipeline and branch to build."
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
  ) : !project.repository_id && canWriteInstanceSettings ? (
    <Button onClick={() => setTab('settings')}>Connect repository</Button>
  ) : projectHasSource && pipelineCount === 0 && canWritePipelines ? (
    <Button
      nativeButton={false}
      render={
        <Link to="/projects/$projectId/pipelines/new" params={{ projectId }} />
      }
    >
      <HugeiconsIcon icon={Add01Icon} />
      Add pipeline
    </Button>
  ) : undefined

  return (
    <PageLayout width="wide" fill>
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
        actions={headerAction}
      />
      {!project.repository_id && !showSetup ? (
        <Alert>
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              This project has no repository.{' '}
              {canWriteInstanceSettings
                ? 'Connect one before running builds.'
                : 'Ask an owner or admin to connect one before running builds.'}
            </span>
            {canWriteInstanceSettings ? (
              <Button
                size="sm"
                variant="outline"
                onClick={() => setTab('settings')}
              >
                Connect repository
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
                <span className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <span>
                    Runners are paused on{' '}
                    <Link
                      to="/settings/runners"
                      className="font-medium underline underline-offset-4"
                    >
                      Runners
                    </Link>
                    . Builds can be queued, but they will not start until you
                    resume them.
                  </span>
                  <ResumeRunnersButton />
                </span>
              ) : (
                <>
                  Runners are paused. Builds can be queued, but they will not
                  start. Ask an owner or admin to resume them.
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

      <Tabs
        value={activeTab}
        onValueChange={(value) => {
          const tabValue = TAB_VALUES.find((candidate) => candidate === value)
          if (tabValue) setTab(tabValue)
        }}
        className="min-h-0 flex-1"
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
                lastBuildByPipeline={lastBuildByPipeline}
                // The Pipelines tab's search and page share this query.
                pipelines={
                  search.pipelineQ || pipelinePage > 1 ? [] : pipelines
                }
                loading={buildSummaryQuery.isLoading}
                error={buildSummaryQuery.error}
                onRetry={() => void buildSummaryQuery.refetch()}
                setup={
                  showSetup ? <SetupChecklist projectId={projectId} /> : null
                }
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

        <TabsContent value="settings">
          <div className="flex flex-col gap-8 pt-2">
            {activeTab === 'settings' ? (
              <Suspense fallback={<Skeleton className="h-48 w-full" />}>
                {canWriteProjects ? (
                  <ProjectSettingsForm
                    canChangeSource={canWriteInstanceSettings}
                    projectId={projectId}
                    currentValues={{
                      name: project.name,
                      description: project.description ?? undefined,
                      default_branch: project.default_branch ?? undefined,
                      repository_id: project.repository_id ?? undefined,
                      repository_full_name:
                        project.repository_full_name ?? undefined,
                    }}
                  />
                ) : (
                  <SettingsSection title="General">
                    <SettingsSurface>
                      <dl className="grid gap-x-6 gap-y-3 text-sm sm:grid-cols-[10rem_1fr]">
                        <dt className="text-muted-foreground">Name</dt>
                        <dd>{project.name}</dd>
                        <dt className="text-muted-foreground">Description</dt>
                        <dd>{project.description || 'None'}</dd>
                        <dt className="text-muted-foreground">
                          Default branch
                        </dt>
                        <dd className="font-mono">
                          {project.default_branch || 'Not set'}
                        </dd>
                        <dt className="text-muted-foreground">Repository</dt>
                        <dd>
                          {project.repository_full_name || 'Not connected'}
                        </dd>
                      </dl>
                    </SettingsSurface>
                  </SettingsSection>
                )}
                {canManageAccess ? (
                  <ProjectAccessCard projectId={projectId} />
                ) : null}
                <ProjectSecretsPanel
                  canWrite={canWritePipelines}
                  inheritedFrom={
                    canReadInstanceSettings ? [{ scope: 'instance' }] : []
                  }
                  target={{ scope: 'project', projectId }}
                  title="Project secrets"
                />
              </Suspense>
            ) : null}

            {canDeleteProjects ? (
              <DangerZone
                title="Delete this project"
                description={`Permanently deletes "${project.name}" and all associated pipelines and builds. This cannot be undone.`}
                action={
                  <Button
                    variant="destructive"
                    onClick={() => setDeleteOpen(true)}
                  >
                    <HugeiconsIcon icon={Delete02Icon} />
                    Delete project
                  </Button>
                }
              />
            ) : null}
          </div>
        </TabsContent>
      </Tabs>

      <ConfirmDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title="Delete project?"
        description={`This will permanently delete "${project.name}" and all associated pipelines and builds. This action cannot be undone.`}
        confirmLabel="Delete project"
        confirmVariant="destructive"
        isPending={deleteMutation.isPending}
        pendingLabel="Deleting..."
        onConfirm={handleDelete}
      />
    </PageLayout>
  )
}
