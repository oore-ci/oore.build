import { useState, useEffect } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  Edit02Icon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { useDeletePipeline, usePipeline, useUpdatePipeline } from '@/hooks/use-pipelines'
import { useProject } from '@/hooks/use-projects'
import { getPipelineStatusVariant, getStatusVariant } from '@/lib/status-variants'
import { webPageTitle } from '@/lib/seo'
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
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import EditPipelineDialog from '../-edit-pipeline-dialog'
import TriggerBuildDialog from '@/components/trigger-build-dialog'

export const Route = createFileRoute(
  '/projects/$projectId/pipelines/$pipelineId',
)({
  staticData: { breadcrumbLabel: 'Pipeline' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: PipelineDetailPage,
})

function relativeTime(epochSecs: number): string {
  const diffSecs = Math.floor(Date.now() / 1000) - epochSecs
  if (diffSecs < 5) return 'just now'
  if (diffSecs < 60) return `${diffSecs}s ago`
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  const days = Math.floor(hrs / 24)
  return `${days}d ago`
}

function PipelineDetailPage() {
  const { projectId, pipelineId } = Route.useParams()
  const navigate = useNavigate()
  const { data, isLoading, error } = usePipeline(pipelineId)
  const { data: projectData } = useProject(projectId)
  const { data: buildsData } = useBuilds({
    pipeline_id: pipelineId,
    limit: 20,
  })
  const updateMutation = useUpdatePipeline()
  const deleteMutation = useDeletePipeline()
  const canWrite = useHasPermission('pipelines', 'write')
  const canDelete = useHasPermission('pipelines', 'delete')
  const canTriggerBuild = useHasPermission('builds', 'write')

  const [editOpen, setEditOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)

  useEffect(() => {
    const label = data?.pipeline.name ?? 'Pipeline Details'
    document.title = webPageTitle(label)
  }, [data?.pipeline.name])

  if (isLoading) {
    return (
      <PageLayout width="wide">
        <Skeleton className="h-8 w-56" />
        <Skeleton className="h-28 w-full" />
        <Skeleton className="h-56 w-full" />
      </PageLayout>
    )
  }

  if (error) {
    return (
      <PageLayout width="wide">
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load pipeline: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { pipeline } = data
  const builds = buildsData?.builds ?? []

  function handleToggleEnabled() {
    updateMutation.mutate(
      {
        pipelineId: pipeline.id,
        data: { enabled: !pipeline.enabled },
      },
      {
        onSuccess: () => {
          toast.success(
            pipeline.enabled ? 'Pipeline disabled' : 'Pipeline enabled',
          )
        },
        onError: (err) => {
          toast.error(`Failed to update pipeline: ${err.message}`)
        },
      },
    )
  }

  function handleDelete() {
    deleteMutation.mutate(pipelineId, {
      onSuccess: () => {
        toast.success('Pipeline deleted')
        void navigate({
          to: '/projects/$projectId',
          params: { projectId },
        })
      },
      onError: (err) => {
        toast.error(`Failed to delete pipeline: ${err.message}`)
      },
    })
  }

  return (
    <PageLayout width="wide">
      <PageHeader
        title={pipeline.name}
        back={{
          to: `/projects/${projectId}`,
          label: 'Project',
        }}
        description="Pipeline execution and trigger policy overview."
        meta={
          <>
            <Badge variant={getPipelineStatusVariant(pipeline.enabled)}>
              {pipeline.enabled ? 'enabled' : 'disabled'}
            </Badge>
            <span>{pipeline.config_path_explicit ? 'explicit config path' : 'auto-detect config'}</span>
            {pipeline.config_path_explicit ? (
              <span className="font-mono">{pipeline.config_path}</span>
            ) : null}
            <span>Updated {relativeTime(pipeline.updated_at)}</span>
          </>
        }
        actions={
          canWrite || canDelete || canTriggerBuild ? (
            <>
              {canTriggerBuild ? (
                <Button onClick={() => setTriggerBuildOpen(true)}>
                  Trigger Build
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
                <Button variant="outline" onClick={() => setEditOpen(true)}>
                  <HugeiconsIcon icon={Edit02Icon} size={16} />
                  Edit
                </Button>
              ) : null}
              {canDelete ? (
                <Button
                  variant="destructive"
                  onClick={() => setDeleteOpen(true)}
                >
                  <HugeiconsIcon icon={Delete02Icon} size={16} />
                  Delete
                </Button>
              ) : null}
            </>
          ) : undefined
        }
      />

      <section className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Recent builds</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-semibold tracking-tight">{builds.length}</p>
            <p className="text-xs text-muted-foreground">Latest 20 runs</p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Cancel previous</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm font-medium">
              {pipeline.concurrency.cancel_previous ? 'enabled' : 'disabled'}
            </p>
            <p className="text-xs text-muted-foreground">Concurrency policy behavior</p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Max concurrent</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm font-medium">{pipeline.concurrency.max_concurrent ?? 'unlimited'}</p>
            <p className="text-xs text-muted-foreground">Per-pipeline limit</p>
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Configuration</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableBody>
              <TableRow>
                <TableCell className="w-56 text-muted-foreground">Name</TableCell>
                <TableCell>{pipeline.name}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Config path</TableCell>
                <TableCell className="font-mono text-xs">{pipeline.config_path}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Config resolution</TableCell>
                <TableCell>
                  {pipeline.config_path_explicit
                    ? 'Explicit path only (UI fallback if file missing)'
                    : 'Auto-detect .oore.yaml then .oore.yml (UI fallback if missing)'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Created</TableCell>
                <TableCell>{new Date(pipeline.created_at * 1000).toLocaleString()}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Updated</TableCell>
                <TableCell>{new Date(pipeline.updated_at * 1000).toLocaleString()}</TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Trigger configuration</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableBody>
              <TableRow>
                <TableCell className="w-56 text-muted-foreground">Events</TableCell>
                <TableCell>
                  {pipeline.trigger_config.events.length > 0 ? (
                    <div className="flex flex-wrap gap-1">
                      {pipeline.trigger_config.events.map((event) => (
                        <Badge key={event} variant="outline" className="text-[11px]">
                          {event}
                        </Badge>
                      ))}
                    </div>
                  ) : (
                    <span className="text-sm text-muted-foreground">all events</span>
                  )}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Branch patterns</TableCell>
                <TableCell>
                  {pipeline.trigger_config.branches.length > 0 ? (
                    <div className="flex flex-wrap gap-1">
                      {pipeline.trigger_config.branches.map((branch) => (
                        <Badge key={branch} variant="outline" className="font-mono text-[11px]">
                          {branch}
                        </Badge>
                      ))}
                    </div>
                  ) : (
                    <span className="text-sm text-muted-foreground">all branches</span>
                  )}
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Fallback execution config</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableBody>
              <TableRow>
                <TableCell className="w-56 text-muted-foreground">Platforms</TableCell>
                <TableCell>
                  {pipeline.execution_config.platforms.length > 0 ? (
                    <div className="flex flex-wrap gap-1">
                      {pipeline.execution_config.platforms.map((platform) => (
                        <Badge key={platform} variant="outline" className="text-[11px]">
                          {platform}
                        </Badge>
                      ))}
                    </div>
                  ) : (
                    <span className="text-sm text-muted-foreground">none</span>
                  )}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Pre-build commands</TableCell>
                <TableCell className="font-mono text-xs">
                  {pipeline.execution_config.commands.pre_build.length > 0
                    ? pipeline.execution_config.commands.pre_build.join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Flutter version</TableCell>
                <TableCell className="font-mono text-xs">
                  {pipeline.execution_config.flutter_version || 'auto (.fvmrc if present)'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Build commands</TableCell>
                <TableCell className="font-mono text-xs">
                  {pipeline.execution_config.commands.build.length > 0
                    ? pipeline.execution_config.commands.build.join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Post-build commands</TableCell>
                <TableCell className="font-mono text-xs">
                  {pipeline.execution_config.commands.post_build.length > 0
                    ? pipeline.execution_config.commands.post_build.join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Android build args</TableCell>
                <TableCell className="font-mono text-xs">
                  {(pipeline.execution_config.platform_build_args?.android?.length ?? 0) > 0
                    ? pipeline.execution_config.platform_build_args?.android.join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">iOS build args</TableCell>
                <TableCell className="font-mono text-xs">
                  {(pipeline.execution_config.platform_build_args?.ios?.length ?? 0) > 0
                    ? pipeline.execution_config.platform_build_args?.ios.join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">macOS build args</TableCell>
                <TableCell className="font-mono text-xs">
                  {(pipeline.execution_config.platform_build_args?.macos?.length ?? 0) > 0
                    ? pipeline.execution_config.platform_build_args?.macos.join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Command overrides</TableCell>
                <TableCell className="font-mono text-xs">
                  {[
                    pipeline.execution_config.platform_commands?.android
                      ? `android: ${pipeline.execution_config.platform_commands.android}`
                      : '',
                    pipeline.execution_config.platform_commands?.ios
                      ? `ios: ${pipeline.execution_config.platform_commands.ios}`
                      : '',
                    pipeline.execution_config.platform_commands?.macos
                      ? `macos: ${pipeline.execution_config.platform_commands.macos}`
                      : '',
                  ]
                    .filter(Boolean)
                    .join(' | ') || 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Environment variables</TableCell>
                <TableCell className="font-mono text-xs">
                  {(pipeline.execution_config.env?.length ?? 0) > 0
                    ? pipeline.execution_config.env
                        ?.map((entry) => `${entry.key}=${entry.value}`)
                        .join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Artifact patterns</TableCell>
                <TableCell className="font-mono text-xs">
                  {pipeline.execution_config.artifact_patterns.length > 0
                    ? pipeline.execution_config.artifact_patterns.join(' | ')
                    : 'none'}
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Recent builds</CardTitle>
        </CardHeader>
        <CardContent>
          {builds.length === 0 ? (
            <div className="space-y-2 py-3">
              <p className="text-sm text-muted-foreground">No builds yet.</p>
              {canTriggerBuild ? (
                <Button size="sm" onClick={() => setTriggerBuildOpen(true)}>
                  Trigger first build
                </Button>
              ) : null}
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Build</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Branch</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead className="text-right">Open</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {builds.map((build) => (
                  <TableRow key={build.id}>
                    <TableCell className="font-mono text-sm">#{build.build_number}</TableCell>
                    <TableCell>
                      <Badge variant={getStatusVariant(build.status)}>{build.status}</Badge>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {build.branch ?? 'n/a'}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {new Date(build.created_at * 1000).toLocaleString()}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        render={<Link to="/builds/$buildId" params={{ buildId: build.id }} />}
                      >
                        Open
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {editOpen ? (
        <EditPipelineDialog
          open={editOpen}
          onOpenChange={setEditOpen}
          pipeline={pipeline}
        />
      ) : null}

      <TriggerBuildDialog
        open={triggerBuildOpen}
        onOpenChange={setTriggerBuildOpen}
        fixedProjectId={projectId}
        fixedPipelineId={pipeline.id}
        fixedPipelineName={pipeline.name}
        defaultBranch={projectData?.project.default_branch}
        description="Run this pipeline now with a branch or pinned commit."
        onBuildCreated={(buildId) => {
          void navigate({
            to: '/builds/$buildId',
            params: { buildId },
          })
        }}
      />

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete pipeline?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{pipeline.name}". This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageLayout>
  )
}
