import { lazy, Suspense, useMemo, useState } from 'react'
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

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { usePipelines, useRepositoryWorkflows } from '@/hooks/use-pipelines'
import { useDeleteProject, useProject } from '@/hooks/use-projects'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { relativeTime } from '@/lib/format-utils'
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
  const { tab } = Route.useSearch()
  const navigate = Route.useNavigate()
  const { data, isLoading, error } = useProject(projectId)
  const { data: pipelinesData } = usePipelines(projectId)
  const { data: summaryBuildsData } = useBuilds(
    { project_id: projectId, limit: 20 },
    { refetchInterval: 15_000 },
  )
  const deleteMutation = useDeleteProject()
  const canWritePipelinesGlobally = useHasPermission('pipelines', 'write')
  const canTriggerBuildGlobally = useHasPermission('builds', 'write')
  const canWriteInstanceSettings = useHasPermission(
    'instance_settings',
    'write',
  )
  const canReadInstanceSettings = useHasPermission('instance_settings', 'read')
  const projectRole = data?.current_user_role ?? data?.project.current_user_role
  const canWriteProjects = hasProjectPermission(
    projectRole,
    'projects',
    'write',
  )
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
  const projectSourceAvailable = Boolean(
    data?.project.repository_id && data.project.repository_full_name,
  )
  const shouldDiscoverWorkflows =
    canWritePipelines &&
    projectSourceAvailable &&
    (pipelinesData?.pipelines.length ?? 0) === 0
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

  const activeTab: TabValue = tab ?? 'pipelines'

  const label = data?.project.name ?? 'Project Details'

  if (isLoading) {
    return { status: 'loading' as const, label }
  }

  if (error) {
    return { status: 'error' as const, label, message: error.message }
  }

  if (!data) return { status: 'missing' as const }

  const { project } = data
  const pipelines = pipelinesData?.pipelines ?? []
  const projectHasSource = projectSourceAvailable
  const runnerPolicyBlockReason =
    project.repository_id && !projectHasSource
      ? ('repository_unavailable' as const)
      : canReadInstanceSettings &&
          preferencesQuery.data?.preferences.direct_macos_runner_paused
        ? ('instance_paused' as const)
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
    triggerBuildOpen,
    triggerPipelineId,
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
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load project: {pageState.message}
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
    triggerBuildOpen,
    triggerPipelineId,
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
                    pipelines.length === 0
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
                    disabled={pipelines.length === 0 || !projectHasSource}
                  >
                    <HugeiconsIcon icon={PlayIcon} />
                    Run build
                  </Button>
                </span>
              ) : null}
              {canDeleteProjects ? (
                <Button
                  variant="destructive"
                  onClick={() => setDeleteOpen(true)}
                >
                  <HugeiconsIcon icon={Delete02Icon} />
                  Delete
                </Button>
              ) : null}
            </>
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

      <Tabs value={activeTab} onValueChange={(val) => setTab(val as TabValue)}>
        <TabsList variant="line">
          <TabsTrigger value="pipelines">
            Pipelines{pipelines.length > 0 ? ` (${pipelines.length})` : ''}
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
          projectHasSource={projectHasSource}
          projectId={projectId}
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
              pipelineCount={pipelines.length}
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
                      description: project.description,
                      default_branch: project.default_branch,
                      repository_id: project.repository_id,
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
                      <HugeiconsIcon
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
