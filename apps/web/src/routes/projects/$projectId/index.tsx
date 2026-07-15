import { useMemo, useState } from 'react'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  InformationCircleIcon,
  PlayIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { usePipelines, useRepositoryWorkflows } from '@/hooks/use-pipelines'
import { useDeleteProject, useProject } from '@/hooks/use-projects'
import { useAuthStore } from '@/stores/auth-store'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'
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
import TriggerBuildDialog from '@/components/trigger-build-dialog'
import { ProjectSettingsForm } from './project-settings-form'
import { ProjectAccessCard } from './-project-access-card'
import { ProjectBuildsTab, ProjectPipelinesTab } from './project-detail-tabs'

const TAB_VALUES = ['pipelines', 'builds', 'settings'] as const
type TabValue = (typeof TAB_VALUES)[number]

function validateTabSearch(search: Record<string, unknown>): {
  tab?: TabValue
} {
  const tab = search.tab
  return {
    tab:
      typeof tab === 'string' && TAB_VALUES.includes(tab as TabValue)
        ? (tab as TabValue)
        : undefined,
  }
}

export const Route = createFileRoute('/projects/$projectId/')({
  staticData: {
    breadcrumbLabel: 'Details',
    breadcrumbParent: { label: 'Projects', to: '/projects' },
  },
  validateSearch: validateTabSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ProjectDetailPage,
})

function useProjectDetailPageState() {
  const { projectId } = Route.useParams()
  const { tab } = Route.useSearch()
  const navigate = useNavigate()
  const { data, isLoading, error } = useProject(projectId)
  const { data: pipelinesData } = usePipelines(projectId)
  const { data: buildsData } = useBuilds(
    { project_id: projectId, limit: 20 },
    { refetchInterval: 15_000 },
  )
  const deleteMutation = useDeleteProject()
  const canWriteProjects = useHasPermission('projects', 'write')
  const canDeleteProjects = useHasPermission('projects', 'delete')
  const canWritePipelines = useHasPermission('pipelines', 'write')
  const canTriggerBuild = useHasPermission('builds', 'write')
  const authRole = useAuthStore((state) => state.user?.role)
  const canManageAccess = authRole === 'owner' || authRole === 'admin'
  const shouldDiscoverWorkflows =
    canWritePipelines &&
    !!data?.project.repository_id &&
    (pipelinesData?.pipelines.length ?? 0) === 0
  const repositoryWorkflowsQuery = useRepositoryWorkflows(
    projectId,
    undefined,
    { enabled: shouldDiscoverWorkflows },
  )

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [dangerOpen, setDangerOpen] = useState(false)
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)
  const [triggerPipelineId, setTriggerPipelineId] = useState<
    string | undefined
  >()

  const builds = useMemo(() => buildsData?.builds ?? [], [buildsData?.builds])
  const { lastBuildByPipeline, latestSucceededBuild } = useMemo(() => {
    const byPipeline = new Map<string, { status: string; time: number }>()
    let latestSucceeded: (typeof builds)[number] | null = null

    for (const build of builds) {
      if (build.pipeline_id && !byPipeline.has(build.pipeline_id)) {
        byPipeline.set(build.pipeline_id, {
          status: build.status,
          time: build.queued_at,
        })
      }
      if (latestSucceeded === null && build.status === 'succeeded') {
        latestSucceeded = build
      }
    }

    return {
      lastBuildByPipeline: byPipeline,
      latestSucceededBuild: latestSucceeded,
    }
  }, [builds])

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
  const projectHasSource = !!project.repository_id

  function setTab(value: TabValue) {
    void navigate({
      to: '/projects/$projectId',
      params: { projectId },
      search: value === 'pipelines' ? {} : { tab: value },
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
    builds,
    canDeleteProjects,
    canManageAccess,
    canTriggerBuild,
    canWritePipelines,
    canWriteProjects,
    dangerOpen,
    deleteMutation,
    deleteOpen,
    handleDelete,
    label,
    lastBuildByPipeline,
    latestSucceededBuild,
    navigate,
    openTriggerBuild,
    pipelines,
    project,
    projectHasSource,
    projectId,
    repositoryWorkflowsQuery,
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
    builds,
    canDeleteProjects,
    canManageAccess,
    canTriggerBuild,
    canWritePipelines,
    canWriteProjects,
    dangerOpen,
    deleteMutation,
    deleteOpen,
    handleDelete,
    label,
    lastBuildByPipeline,
    latestSucceededBuild,
    navigate,
    openTriggerBuild,
    pipelines,
    project,
    projectHasSource,
    projectId,
    repositoryWorkflowsQuery,
    setDangerOpen,
    setDeleteOpen,
    setTab,
    setTriggerBuildOpen,
    setTriggerPipelineId,
    triggerBuildOpen,
    triggerPipelineId,
  } = pageState

  const openBuild = (buildId: string) => {
    void navigate({ to: '/builds/$buildId', params: { buildId } })
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
      {!projectHasSource ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            This project has no linked source repository. Link a repository
            before triggering builds.
          </AlertDescription>
        </Alert>
      ) : null}

      <Tabs value={activeTab} onValueChange={(val) => setTab(val as TabValue)}>
        <TabsList variant="line">
          <TabsTrigger value="pipelines">
            Pipelines{pipelines.length > 0 ? ` (${pipelines.length})` : ''}
          </TabsTrigger>
          <TabsTrigger value="builds">
            Builds{builds.length > 0 ? ` (${builds.length})` : ''}
          </TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
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
          pipelines={pipelines}
          projectHasSource={projectHasSource}
          projectId={projectId}
          workflowDiscoveryFailed={!!repositoryWorkflowsQuery.error}
          workflowDiscoveryLoading={repositoryWorkflowsQuery.isLoading}
        />

        <ProjectBuildsTab
          builds={builds}
          canTriggerBuild={canTriggerBuild}
          latestSucceededBuild={latestSucceededBuild}
          onOpenBuild={openBuild}
          onTriggerBuild={() => openTriggerBuild()}
          pipelineCount={pipelines.length}
          projectHasSource={projectHasSource}
        />

        {/* ---- Settings tab ---- */}
        <TabsContent value="settings">
          <div className="space-y-4 pt-2">
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
                }}
              />
            ) : (
              <Card>
                <CardContent className="text-sm text-muted-foreground">
                  You do not have permission to edit this project.
                </CardContent>
              </Card>
            )}

            {canDeleteProjects ? (
              <Collapsible open={dangerOpen} onOpenChange={setDangerOpen}>
                <Card className="border-destructive/40">
                  <CardContent>
                    <CollapsibleTrigger className="flex w-full items-center justify-between text-sm font-medium text-destructive">
                      Danger zone
                      <span className="text-xs text-muted-foreground">
                        {dangerOpen ? 'collapse' : 'expand'}
                      </span>
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
      <TriggerBuildDialog
        open={triggerBuildOpen}
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
