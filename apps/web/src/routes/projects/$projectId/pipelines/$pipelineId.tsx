import { lazy, Suspense, useState } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  Edit02Icon,
  InformationCircleIcon,
  PlayIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import {
  getActiveInstanceOrRedirect,
  requireInstanceRoleOrRedirect,
} from '@/lib/instance-context'
import { useBreadcrumbLabel } from '@/hooks/use-breadcrumb-label'
import { useBuilds } from '@/hooks/use-builds'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import {
  useDeletePipeline,
  usePipeline,
  usePipelineAndroidSigning,
  usePipelineIosSigning,
  useUpdatePipeline,
} from '@/hooks/use-pipelines'
import { useProject } from '@/hooks/use-projects'
import {
  getPipelineStatusVariant,
  getStatusVariant,
} from '@/lib/status-variants'
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
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { PipelineConfigurationCard } from './-pipeline-configuration-card'

const loadTriggerBuildDialog = () => import('@/components/trigger-build-dialog')
const TriggerBuildDialog = lazy(loadTriggerBuildDialog)

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId',
)({
  staticData: {
    breadcrumbLabel: 'Pipeline',
    breadcrumbParent: { label: 'Project', to: '/projects/$projectId' },
  },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireInstanceRoleOrRedirect(instance.id, ['owner', 'admin', 'developer'])
  },
  component: PipelineDetailPage,
})

/* ------------------------------------------------------------------ */
/*  Main page                                                          */
/* ------------------------------------------------------------------ */

function usePipelineDetailPageState() {
  const { projectId, pipelineId } = Route.useParams()
  const navigate = useNavigate()
  const { data, isLoading, error } = usePipeline(pipelineId)
  const canWriteGlobally = useHasPermission('pipelines', 'write')
  const canTriggerBuildGlobally = useHasPermission('builds', 'write')
  const { data: projectData } = useProject(projectId)
  const projectRole =
    projectData?.current_user_role ?? projectData?.project.current_user_role
  const canWrite =
    canWriteGlobally && hasProjectPermission(projectRole, 'pipelines', 'write')
  const canDelete = hasProjectPermission(projectRole, 'pipelines', 'delete')
  const canTriggerBuild =
    canTriggerBuildGlobally &&
    hasProjectPermission(projectRole, 'builds', 'write')
  const signingQuery = usePipelineAndroidSigning(pipelineId, {
    enabled: canWrite,
  })
  const iosSigningQuery = usePipelineIosSigning(pipelineId, {
    enabled: canWrite,
  })
  const { data: buildsData } = useBuilds({
    pipeline_id: pipelineId,
    limit: 20,
  })
  const updateMutation = useUpdatePipeline()
  const deleteMutation = useDeletePipeline()

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)

  const label = data?.pipeline.name ?? 'Pipeline Details'

  useBreadcrumbLabel(
    '/projects/$projectId/pipelines/$pipelineId',
    data?.pipeline.name,
  )

  if (isLoading) {
    return { status: 'loading' as const, label }
  }

  if (error) {
    return { status: 'error' as const, label, message: error.message }
  }

  if (!data) return { status: 'missing' as const }

  const { pipeline } = data
  const builds = buildsData?.builds ?? []
  const projectHasSource = !!projectData?.project.repository_id
  const manualOnlyTriggers =
    projectData?.project.repository_provider === 'local_git'

  function handleToggleEnabled() {
    updateMutation.mutate(
      { pipelineId: pipeline.id, data: { enabled: !pipeline.enabled } },
      {
        onSuccess: () =>
          toast.success(
            pipeline.enabled ? 'Pipeline disabled' : 'Pipeline enabled',
          ),
        onError: (err) =>
          toast.error(`Failed to update pipeline: ${err.message}`),
      },
    )
  }

  function handleDelete() {
    deleteMutation.mutate(pipelineId, {
      onSuccess: () => {
        toast.success('Pipeline deleted')
        void navigate({ to: '/projects/$projectId', params: { projectId } })
      },
      onError: (err) =>
        toast.error(`Failed to delete pipeline: ${err.message}`),
    })
  }

  return {
    status: 'ready' as const,
    builds,
    canDelete,
    canTriggerBuild,
    canWrite,
    deleteMutation,
    deleteOpen,
    handleDelete,
    handleToggleEnabled,
    iosSigningQuery,
    label,
    manualOnlyTriggers,
    navigate,
    pipeline,
    pipelineId,
    projectData,
    projectHasSource,
    projectId,
    setDeleteOpen,
    setTriggerBuildOpen,
    signingQuery,
    triggerBuildOpen,
    updateMutation,
  }
}

function PipelineDetailPage() {
  const pageState = usePipelineDetailPageState()

  if (pageState.status === 'loading') {
    return (
      <PageLayout width="wide">
        <PageMeta title={pageState.label} noindex />
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-28 w-full" />
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
            Failed to load pipeline: {pageState.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (pageState.status === 'missing') return null

  const {
    builds,
    canDelete,
    canTriggerBuild,
    canWrite,
    deleteMutation,
    deleteOpen,
    handleDelete,
    handleToggleEnabled,
    iosSigningQuery,
    label,
    manualOnlyTriggers,
    navigate,
    pipeline,
    pipelineId,
    projectData,
    projectHasSource,
    projectId,
    setDeleteOpen,
    setTriggerBuildOpen,
    signingQuery,
    triggerBuildOpen,
    updateMutation,
  } = pageState

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
      <PageHeader
        title={pipeline.name}
        description="Pipeline overview and configuration."
        meta={
          <>
            <Badge variant={getPipelineStatusVariant(pipeline.enabled)}>
              {pipeline.enabled ? 'enabled' : 'disabled'}
            </Badge>
            {pipeline.execution_config.platforms.map((p) => (
              <Badge key={p} variant="outline" className="text-[11px]">
                {p}
              </Badge>
            ))}
            <span>Updated {relativeTime(pipeline.updated_at)}</span>
          </>
        }
        actions={
          canWrite || canDelete || canTriggerBuild ? (
            <>
              {canTriggerBuild ? (
                <Button
                  onMouseEnter={() => void loadTriggerBuildDialog()}
                  onFocus={() => void loadTriggerBuildDialog()}
                  onClick={() => setTriggerBuildOpen(true)}
                  disabled={!projectHasSource}
                >
                  <HugeiconsIcon icon={PlayIcon} />
                  Run build
                </Button>
              ) : null}
              {canWrite ? (
                <Button
                  variant="outline"
                  onClick={handleToggleEnabled}
                  disabled={updateMutation.isPending}
                >
                  {pipeline.enabled ? 'Disable' : 'Enable'}
                </Button>
              ) : null}
              {canWrite ? (
                <Button
                  variant="outline"
                  render={
                    <Link
                      to="/projects/$projectId/pipelines/$pipelineId/edit"
                      params={{ projectId, pipelineId }}
                      search={{}}
                    />
                  }
                  nativeButton={false}
                >
                  <HugeiconsIcon icon={Edit02Icon} />
                  Edit
                </Button>
              ) : null}
              {canDelete ? (
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

      <PipelineConfigurationCard
        androidSigning={signingQuery.data}
        iosSigning={iosSigningQuery.data}
        manualOnlyTriggers={manualOnlyTriggers}
        pipeline={pipeline}
      />
      {/* Recent builds */}
      <Card>
        <CardContent>
          <h3 className="pb-3 text-sm font-medium">Recent builds</h3>
          {builds.length === 0 ? (
            <Empty className="p-8">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <HugeiconsIcon icon={PlayIcon} />
                </EmptyMedia>
                <EmptyTitle>No builds yet</EmptyTitle>
                <EmptyDescription>
                  Run this pipeline to see its status, output, and artifacts
                  here.
                </EmptyDescription>
              </EmptyHeader>
              {canTriggerBuild && projectHasSource ? (
                <EmptyContent>
                  <Button
                    size="sm"
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
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Build</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Branch</TableHead>
                  <TableHead>Created</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {builds.map((build) => (
                  <TableRow
                    key={build.id}
                    className="group cursor-pointer"
                    role="link"
                    tabIndex={0}
                    onClick={() =>
                      void navigate({
                        to: '/builds/$buildId',
                        params: { buildId: build.id },
                      })
                    }
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault()
                        void navigate({
                          to: '/builds/$buildId',
                          params: { buildId: build.id },
                        })
                      }
                    }}
                  >
                    <TableCell className="font-mono text-sm group-hover:underline">
                      #{build.build_number}
                    </TableCell>
                    <TableCell>
                      <Badge variant={getStatusVariant(build.status)}>
                        {build.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {build.branch ?? 'n/a'}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {new Date(build.created_at * 1000).toLocaleString()}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Dialogs */}
      {triggerBuildOpen ? (
        <Suspense fallback={null}>
          <TriggerBuildDialog
            open
            onOpenChange={setTriggerBuildOpen}
            fixedProjectId={projectId}
            fixedPipelineId={pipeline.id}
            fixedPipelineName={pipeline.name}
            defaultBranch={projectData?.project.default_branch}
            description="Run this pipeline now with a branch or pinned commit."
            onBuildCreated={(buildId) => {
              void navigate({ to: '/builds/$buildId', params: { buildId } })
            }}
          />
        </Suspense>
      ) : null}

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete pipeline?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{pipeline.name}". This action cannot
              be undone.
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
