import { lazy, Suspense, useMemo, useState } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  ArrowDown as ArrowDown01Icon,
  ArrowRight as ArrowRight01Icon,
  Trash2 as Delete02Icon,
  Info as InformationCircleIcon,
  Play as PlayIcon,
} from 'lucide-react'
import { toast } from '@/lib/toast'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { usePageClamp } from '@/hooks/use-page-clamp'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { usePipelines, useRepositoryWorkflows } from '@/hooks/use-pipelines'
import { useDeleteProject, useProject } from '@/hooks/use-projects'
import { useSourceRepositories } from '@/hooks/use-source-repositories'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { relativeTime } from '@/lib/format-utils'
import { ApiClientError } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import type { SortDirection } from '@/components/collection-controls'
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
import { PROJECT_BUILD_SORT_OPTIONS } from './-project-build-sort'
import type { ProjectBuildSort } from './-project-build-sort'

const loadTriggerBuildDialog = () => import('@/components/trigger-build-dialog')
const TriggerBuildDialog = lazy(loadTriggerBuildDialog)
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

const TAB_VALUES = ['pipelines', 'builds', 'settings'] as const
type TabValue = (typeof TAB_VALUES)[number]

interface ProjectDetailSearch {
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

const PROJECT_BUILD_SORT_VALUES = new Set<ProjectBuildSort>(
  Object.keys(PROJECT_BUILD_SORT_OPTIONS) as Array<ProjectBuildSort>,
)

function validateProjectSearch(
  search: Record<string, unknown>,
): ProjectDetailSearch {
  const tab = search.tab
  const page = Number(search.page)
  const pageSize = Number(search.pageSize)
  const q = typeof search.q === 'string' ? search.q.trim() : ''
  const status =
    typeof search.status === 'string' &&
    search.status in BUILD_STATUS_FILTER_OPTIONS
      ? search.status
      : ''
  const sort = search.sort as ProjectBuildSort
  const pipelinePage = Number(search.pipelinePage)
  const pipelinePageSize = Number(search.pipelinePageSize)
  const pipelineQ =
    typeof search.pipelineQ === 'string' ? search.pipelineQ.trim() : ''

  return {
    tab:
      typeof tab === 'string' && TAB_VALUES.includes(tab as TabValue)
        ? (tab as TabValue)
        : undefined,
    q: q || undefined,
    status: status && status !== 'all' ? status : undefined,
    sort: PROJECT_BUILD_SORT_VALUES.has(sort) ? sort : undefined,
    direction: search.direction === 'asc' ? 'asc' : undefined,
    page: Number.isInteger(page) && page > 1 ? page : undefined,
    pageSize: pageSize === 50 || pageSize === 100 ? pageSize : undefined,
    pipelineQ: pipelineQ || undefined,
    pipelineSort: search.pipelineSort === 'name' ? 'name' : undefined,
    pipelineDirection: search.pipelineDirection === 'asc' ? 'asc' : undefined,
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
  staticData: {
    breadcrumbLabel: 'Details',
    breadcrumbParent: { label: 'Projects', to: '/projects' },
  },
  validateSearch: validateProjectSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: ProjectDetailPage,
})

function useProjectDetailPageState() {
  const { projectId } = Route.useParams()
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
  const { data: summaryBuildsData } = useBuilds(
    { project_id: projectId, limit: 20 },
    { refetchInterval: 15_000 },
  )
  const deleteMutation = useDeleteProject()
  const canWriteProjectsGlobally = useHasPermission('projects', 'write')
  const canWritePipelinesGlobally = useHasPermission('pipelines', 'write')
  const canTriggerBuildGlobally = useHasPermission('builds', 'write')
  const canWriteInstanceSettings = useHasPermission(
    'instance_settings',
    'write',
  )
  const projectRole = data?.project.current_user_role
  const canWriteProjects =
    canWriteProjectsGlobally &&
    hasProjectPermission(projectRole, 'projects', 'write')
  const canDeleteProjects = hasProjectPermission(
    projectRole,
    'projects',
    'delete',
  )
  const canWritePipelines =
    canWritePipelinesGlobally &&
    hasProjectPermission(projectRole, 'pipelines', 'write')
  const canTriggerBuild =
    canTriggerBuildGlobally &&
    hasProjectPermission(projectRole, 'builds', 'write')
  const canManageAccess = projectRole === 'maintainer'
  const pipelineCount = search.pipelineQ
    ? (data?.pipeline_count ?? 0)
    : (pipelinesData?.total ?? data?.pipeline_count ?? 0)
  const shouldDiscoverWorkflows =
    canWritePipelines && !!data?.project.repository_id && pipelineCount === 0
  const repositoryWorkflowsQuery = useRepositoryWorkflows(
    projectId,
    undefined,
    { enabled: shouldDiscoverWorkflows },
  )
  const sourceRepositoriesQuery = useSourceRepositories(
    !!data?.project.repository_id,
  )
  const preferencesQuery = useInstancePreferences({
    enabled: canWriteInstanceSettings && !!data?.project.repository_id,
  })

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [dangerOpen, setDangerOpen] = useState(false)
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)
  const [triggerPipelineId, setTriggerPipelineId] = useState<
    string | undefined
  >()

  const summaryBuilds = useMemo(
    () => summaryBuildsData?.builds ?? [],
    [summaryBuildsData?.builds],
  )
  const lastBuildByPipeline = useMemo(() => {
    const byPipeline = new Map<string, { status: string; time: number }>()

    for (const build of summaryBuilds) {
      if (build.pipeline_id && !byPipeline.has(build.pipeline_id)) {
        byPipeline.set(build.pipeline_id, {
          status: build.status,
          time: build.queued_at,
        })
      }
    }

    return byPipeline
  }, [summaryBuilds])
  const buildCount = summaryBuildsData?.total ?? data?.build_count ?? 0

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

  const activeTab: TabValue = tab ?? 'pipelines'

  const label = data?.project.name ?? 'Project Details'

  if (isLoading) {
    return { status: 'loading' as const, label }
  }

  if (error) {
    return {
      status: 'error' as const,
      label,
      message: error.message,
      notFound: error instanceof ApiClientError && error.status === 404,
      retry: projectQuery.refetch,
    }
  }

  if (!data) return { status: 'missing' as const }

  const { project } = data
  const pipelines = pipelinesData?.pipelines ?? []
  const projectHasSource = !!project.repository_id
  const sourceRepository = sourceRepositoriesQuery.data?.find(
    (repository) => repository.id === project.repository_id,
  )
  const runnerPolicyBlockReason = !preferencesQuery.data
    ? undefined
    : !preferencesQuery.data.preferences.direct_macos_runner_enabled
      ? ('instance_disabled' as const)
      : sourceRepository && !sourceRepository.allow_direct_macos_runner
        ? ('repository_not_approved' as const)
        : sourceRepositoriesQuery.isSuccess && !sourceRepository
          ? ('repository_unavailable' as const)
          : undefined

  function setTab(value: TabValue) {
    void navigate({
      to: '/projects/$projectId',
      params: { projectId },
      search: (previous) => ({
        ...previous,
        tab: value === 'pipelines' ? undefined : value,
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
    setTriggerPipelineId(() => pipelineId)
    setTriggerBuildOpen(true)
  }

  return {
    status: 'ready' as const,
    activeTab,
    buildCount,
    canDeleteProjects,
    canManageAccess,
    canTriggerBuild,
    canWriteInstanceSettings,
    canWritePipelines,
    canWriteProjects,
    dangerOpen,
    deleteMutation,
    deleteOpen,
    handleDelete,
    label,
    lastBuildByPipeline,
    navigate,
    openTriggerBuild,
    pipelines,
    pipelinesQuery,
    pipelineDirection,
    pipelineCount,
    pipelinePage,
    pipelinePageSize,
    pipelineQuery: search.pipelineQ ?? '',
    pipelineSort,
    project,
    projectHasSource,
    projectId,
    repositoryWorkflowsQuery,
    runnerPolicyBlockReason,
    setDangerOpen,
    setDeleteOpen,
    setTab,
    setTriggerBuildOpen,
    setTriggerPipelineId,
    sourceRepository,
    triggerBuildOpen,
    triggerPipelineId,
    updatePipelineSearch,
  }
}

function ProjectDetailPage() {
  const pageState = useProjectDetailPageState()

  if (pageState.status === 'loading') {
    return (
      <PageLayout width="wide">
        <PageMeta title={pageState.label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-10 w-72" />
        <Skeleton className="h-56 w-full" />
      </PageLayout>
    )
  }

  if (pageState.status === 'error') {
    return (
      <PageLayout width="wide">
        <PageMeta title={pageState.label} noindex />
        <Alert variant="destructive">
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              {pageState.notFound
                ? 'This project was not found or is no longer available.'
                : `Failed to load project: ${pageState.message}`}
            </span>
            <span className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                render={<Link to="/projects" />}
              >
                Back to projects
              </Button>
              {!pageState.notFound ? (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void pageState.retry()}
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

  if (pageState.status === 'missing') return null

  const {
    activeTab,
    buildCount,
    canDeleteProjects,
    canManageAccess,
    canTriggerBuild,
    canWriteInstanceSettings,
    canWritePipelines,
    canWriteProjects,
    dangerOpen,
    deleteMutation,
    deleteOpen,
    handleDelete,
    label,
    lastBuildByPipeline,
    navigate,
    openTriggerBuild,
    pipelines,
    pipelinesQuery,
    pipelineDirection,
    pipelineCount,
    pipelinePage,
    pipelinePageSize,
    pipelineQuery,
    pipelineSort,
    project,
    projectHasSource,
    projectId,
    repositoryWorkflowsQuery,
    runnerPolicyBlockReason,
    setDangerOpen,
    setDeleteOpen,
    setTab,
    setTriggerBuildOpen,
    setTriggerPipelineId,
    sourceRepository,
    triggerBuildOpen,
    triggerPipelineId,
    updatePipelineSearch,
  } = pageState

  function preloadProjectSettings() {
    if (canManageAccess) void loadProjectAccessCard()
    if (canWriteProjects) void loadProjectSettingsForm()
  }

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
      <PageHeader
        title={project.name}
        description={project.description}
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
          canTriggerBuild || canDeleteProjects ? (
            <>
              {canTriggerBuild ? (
                <span
                  title={
                    pipelineCount === 0
                      ? 'Add a pipeline first before running builds'
                      : !projectHasSource
                        ? 'Connect a source repository first'
                        : undefined
                  }
                >
                  <Button
                    onMouseEnter={() => void loadTriggerBuildDialog()}
                    onFocus={() => void loadTriggerBuildDialog()}
                    onClick={() => openTriggerBuild()}
                    disabled={pipelineCount === 0 || !projectHasSource}
                  >
                    <DynamicLucideIcon icon={PlayIcon} />
                    Run build
                  </Button>
                </span>
              ) : null}
              {canDeleteProjects ? (
                <Button
                  variant="destructive"
                  onClick={() => setDeleteOpen(true)}
                >
                  <DynamicLucideIcon icon={Delete02Icon} />
                  Delete
                </Button>
              ) : null}
            </>
          ) : undefined
        }
      />
      {!projectHasSource ? (
        <Alert variant="destructive">
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            This project has no linked source repository. Link a repository
            before triggering builds.{' '}
            {canWriteProjects ? (
              <Link
                to="/projects/$projectId"
                params={{ projectId }}
                search={{ tab: 'settings' }}
                className="font-medium underline underline-offset-4"
              >
                Open project settings
              </Link>
            ) : (
              'Ask a project maintainer to relink it.'
            )}
          </AlertDescription>
        </Alert>
      ) : null}
      {projectHasSource && runnerPolicyBlockReason ? (
        <Alert>
          <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            {runnerPolicyBlockReason === 'instance_disabled' ? (
              <>
                Direct macOS runner is paused. Builds can be queued, but they
                will not start until an owner or admin enables it
                {canWriteInstanceSettings ? (
                  <>
                    {' in '}
                    <Link
                      to="/settings/runners"
                      className="font-medium underline underline-offset-4"
                    >
                      Runners
                    </Link>
                  </>
                ) : null}
                .
              </>
            ) : runnerPolicyBlockReason === 'repository_not_approved' ? (
              <>
                This repository is not approved for Direct runner builds. Builds
                will remain queued until an owner or admin approves it
                {sourceRepository ? (
                  <>
                    {' in '}
                    <Link
                      to="/settings/integrations/$integrationId"
                      params={{
                        integrationId: sourceRepository.integration_id,
                      }}
                      className="font-medium underline underline-offset-4"
                    >
                      Sources
                    </Link>
                  </>
                ) : null}
                .
              </>
            ) : (
              <>
                Oore cannot find this project&apos;s repository policy. Builds
                fail closed and remain queued. Check the repository under{' '}
                <Link
                  to="/settings/integrations"
                  className="font-medium underline underline-offset-4"
                >
                  Sources
                </Link>
                .
              </>
            )}
          </AlertDescription>
        </Alert>
      ) : null}

      <Tabs value={activeTab} onValueChange={(val) => setTab(val as TabValue)}>
        <TabsList variant="line">
          <TabsTrigger value="pipelines">
            Pipelines
            {pipelineCount > 0 ? ` (${pipelineCount})` : ''}
          </TabsTrigger>
          <TabsTrigger value="builds">
            Builds{buildCount > 0 ? ` (${buildCount})` : ''}
          </TabsTrigger>
          <TabsTrigger
            value="settings"
            onMouseEnter={preloadProjectSettings}
            onFocus={preloadProjectSettings}
          >
            Settings
          </TabsTrigger>
        </TabsList>

        <ProjectPipelinesTab
          canTriggerBuild={canTriggerBuild}
          canWritePipelines={canWritePipelines}
          defaultBranch={project.default_branch}
          hasValidRepositoryWorkflow={
            repositoryWorkflowsQuery.data?.workflows.some(
              (workflow) => workflow.valid,
            ) ?? false
          }
          lastBuildByPipeline={lastBuildByPipeline}
          onPreloadTriggerBuild={() => void loadTriggerBuildDialog()}
          onTriggerBuild={openTriggerBuild}
          pipelines={pipelines}
          direction={pipelineDirection}
          error={pipelinesQuery.error?.message}
          isLoading={pipelinesQuery.isLoading}
          onDirectionChange={(direction) =>
            updatePipelineSearch({
              pipelineDirection: direction === 'desc' ? undefined : direction,
              pipelinePage: undefined,
            })
          }
          onPageChange={(page) =>
            updatePipelineSearch({
              pipelinePage: page === 1 ? undefined : page,
            })
          }
          onPageSizeChange={(pageSize) =>
            updatePipelineSearch({
              pipelinePage: undefined,
              pipelinePageSize: pageSize === 20 ? undefined : pageSize,
            })
          }
          onQueryChange={(query) =>
            updatePipelineSearch({
              pipelineQ: query.trim() || undefined,
              pipelinePage: undefined,
            })
          }
          onRetry={() => void pipelinesQuery.refetch()}
          onSortChange={(sort) =>
            updatePipelineSearch({
              pipelineSort: sort === 'created_at' ? undefined : sort,
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
              active
              canTriggerBuild={canTriggerBuild}
              onPreloadTriggerBuild={() => void loadTriggerBuildDialog()}
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
                    projectId={projectId}
                    currentValues={{
                      name: project.name,
                      description: project.description,
                      default_branch: project.default_branch,
                      repository_id: project.repository_id,
                      repository_full_name: project.repository_full_name,
                    }}
                  />
                ) : (
                  <Card>
                    <CardContent className="text-sm text-muted-foreground">
                      You do not have permission to edit this project.
                    </CardContent>
                  </Card>
                )}
              </Suspense>
            ) : null}

            {canDeleteProjects ? (
              <Collapsible open={dangerOpen} onOpenChange={setDangerOpen}>
                <Card className="border-destructive/40">
                  <CardContent>
                    <CollapsibleTrigger className="flex w-full items-center justify-between text-sm font-medium text-destructive">
                      Danger zone
                      <DynamicLucideIcon
                        icon={dangerOpen ? ArrowDown01Icon : ArrowRight01Icon}
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
                          <DynamicLucideIcon icon={Delete02Icon} />
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

      {/* Dialogs */}
      {triggerBuildOpen ? (
        <Suspense fallback={null}>
          <TriggerBuildDialog
            open
            onOpenChange={(nextOpen) => {
              setTriggerBuildOpen(() => nextOpen)
              if (!nextOpen) setTriggerPipelineId(undefined)
            }}
            fixedProjectId={projectId}
            defaultPipelineId={triggerPipelineId}
            defaultBranch={project.default_branch}
            description="Run this project's pipeline now."
            onBuildCreated={(buildId) => {
              void navigate({ to: '/builds/$buildId', params: { buildId } })
            }}
          />
        </Suspense>
      ) : null}

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
