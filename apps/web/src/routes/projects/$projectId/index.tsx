import { useState, useEffect } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Delete02Icon,
  Edit02Icon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { usePipelines } from '@/hooks/use-pipelines'
import { useDeleteProject, useProject, useUpdateProject } from '@/hooks/use-projects'
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import CreatePipelineDialog from './-create-pipeline-dialog'
import TriggerBuildDialog from '@/components/trigger-build-dialog'

export const Route = createFileRoute('/projects/$projectId/')({
  staticData: { breadcrumbLabel: 'Details' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ProjectDetailPage,
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

const editProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
})

type EditProjectForm = z.infer<typeof editProjectSchema>

function EditProjectDialog({
  open,
  onOpenChange,
  projectId,
  currentValues,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectId: string
  currentValues: { name: string; description?: string; default_branch?: string }
}) {
  const updateMutation = useUpdateProject()

  const form = useForm<EditProjectForm>({
    resolver: zodResolver(editProjectSchema),
    defaultValues: {
      name: currentValues.name,
      description: currentValues.description ?? '',
      default_branch: currentValues.default_branch ?? '',
    },
    mode: 'onBlur',
  })

  useEffect(() => {
    if (open) {
      form.reset({
        name: currentValues.name,
        description: currentValues.description ?? '',
        default_branch: currentValues.default_branch ?? '',
      })
    }
  }, [open, currentValues, form])

  function onSubmit(data: EditProjectForm) {
    updateMutation.mutate(
      {
        projectId,
        data: {
          name: data.name.trim(),
          description: data.description?.trim() || undefined,
          default_branch: data.default_branch?.trim() || undefined,
        },
      },
      {
        onSuccess: () => {
          toast.success('Project updated')
          onOpenChange(false)
        },
        onError: (err) => {
          toast.error(`Failed to update project: ${err.message}`)
        },
      },
    )
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit Project</DialogTitle>
          <DialogDescription>Update project settings.</DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="name"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Name</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="description"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Description</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="default_branch"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Default Branch</FormLabel>
                  <FormControl>
                    <Input {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={updateMutation.isPending}>
                {updateMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Saving...
                  </>
                ) : (
                  'Save'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}

function ProjectDetailPage() {
  const { projectId } = Route.useParams()
  const navigate = useNavigate()
  const { data, isLoading, error } = useProject(projectId)
  const { data: pipelinesData } = usePipelines(projectId)
  const { data: buildsData } = useBuilds({ project_id: projectId, limit: 10 })
  const deleteMutation = useDeleteProject()
  const canWriteProjects = useHasPermission('projects', 'write')
  const canDeleteProjects = useHasPermission('projects', 'delete')
  const canWritePipelines = useHasPermission('pipelines', 'write')
  const canTriggerBuild = useHasPermission('builds', 'write')

  const [editOpen, setEditOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [pipelineCreateOpen, setPipelineCreateOpen] = useState(false)
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)
  const [triggerPipelineId, setTriggerPipelineId] = useState<string | undefined>()

  useEffect(() => {
    const label = data?.project.name ?? 'Project Details'
    document.title = webPageTitle(label)
  }, [data?.project.name])

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
            Failed to load project: {error.message}
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!data) return null

  const { project } = data
  const pipelines = pipelinesData?.pipelines ?? []
  const builds = buildsData?.builds ?? []

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
    setTriggerPipelineId(pipelineId)
    setTriggerBuildOpen(true)
  }

  return (
    <PageLayout width="wide">
      <PageHeader
        title={project.name}
        back={{ to: '/projects', label: 'Projects' }}
        description={project.description}
        meta={
          <>
            {project.default_branch ? (
              <Badge variant="outline" className="font-mono text-[11px]">
                {project.default_branch}
              </Badge>
            ) : null}
            <span>Updated {relativeTime(project.updated_at)}</span>
            <span className="font-mono">{project.id.slice(0, 8)}</span>
          </>
        }
        actions={
          canWriteProjects || canDeleteProjects || canTriggerBuild ? (
            <>
              {canTriggerBuild ? (
                <Button
                  onClick={() => openTriggerBuild()}
                  disabled={pipelines.length === 0}
                >
                  Trigger Build
                </Button>
              ) : null}
              {canWriteProjects ? (
                <Button variant="outline" onClick={() => setEditOpen(true)}>
                  <HugeiconsIcon icon={Edit02Icon} size={16} />
                  Edit
                </Button>
              ) : null}
              {canDeleteProjects ? (
                <Button variant="destructive" onClick={() => setDeleteOpen(true)}>
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
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Pipelines</p>
            <p className="mt-3 text-2xl font-bold tracking-tight">{pipelines.length}</p>
            <p className="mt-1 text-xs text-muted-foreground">Configured under this project</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Recent builds</p>
            <p className="mt-3 text-2xl font-bold tracking-tight">{builds.length}</p>
            <p className="mt-1 text-xs text-muted-foreground">Latest 10 runs</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent>
            <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Created by</p>
            <p className="mt-3 truncate text-sm font-bold">{project.created_by}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {new Date(project.created_at * 1000).toLocaleString()}
            </p>
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">Project details</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableBody>
              <TableRow>
                <TableCell className="w-56 text-muted-foreground">Name</TableCell>
                <TableCell>{project.name}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Description</TableCell>
                <TableCell>{project.description ?? 'No description'}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Default branch</TableCell>
                <TableCell className="font-mono text-xs">{project.default_branch ?? 'not set'}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell className="text-muted-foreground">Updated</TableCell>
                <TableCell>{new Date(project.updated_at * 1000).toLocaleString()}</TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">Pipelines</CardTitle>
            {canWritePipelines ? (
              <Button size="sm" onClick={() => setPipelineCreateOpen(true)}>
                <HugeiconsIcon icon={Add01Icon} size={14} />
                Add Pipeline
              </Button>
            ) : null}
          </div>
        </CardHeader>
        <CardContent>
          {pipelines.length === 0 ? (
            <p className="py-3 text-sm text-muted-foreground">No pipelines yet. Add one to start building.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Config path</TableHead>
                  <TableHead>Triggers</TableHead>
                  <TableHead>Updated</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {pipelines.map((pipeline) => (
                  <TableRow key={pipeline.id}>
                    <TableCell className="font-medium">{pipeline.name}</TableCell>
                    <TableCell>
                      <Badge variant={getPipelineStatusVariant(pipeline.enabled)}>
                        {pipeline.enabled ? 'enabled' : 'disabled'}
                      </Badge>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {pipeline.config_path}
                    </TableCell>
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
                        <span className="text-xs text-muted-foreground">all events</span>
                      )}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {relativeTime(pipeline.updated_at)}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="inline-flex items-center gap-2">
                        {canTriggerBuild ? (
                          <Button
                            size="sm"
                            onClick={() => openTriggerBuild(pipeline.id)}
                          >
                            Run
                          </Button>
                        ) : null}
                        <Button
                          variant="outline"
                          size="sm"
                          render={
                            <Link
                              to="/projects/$projectId/pipelines/$pipelineId"
                              params={{ projectId, pipelineId: pipeline.id }}
                            />
                          }
                        >
                          Open
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">Recent builds</CardTitle>
        </CardHeader>
        <CardContent>
          {builds.length === 0 ? (
            <div className="space-y-2 py-3">
              <p className="text-sm text-muted-foreground">No builds yet.</p>
              {canTriggerBuild && pipelines.length > 0 ? (
                <Button size="sm" onClick={() => openTriggerBuild()}>
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
                  <TableHead>Trigger</TableHead>
                  <TableHead>Branch</TableHead>
                  <TableHead>Queued</TableHead>
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
                    <TableCell>
                      <Badge variant="outline">{build.trigger_type}</Badge>
                    </TableCell>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {build.branch ?? 'n/a'}
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {relativeTime(build.queued_at)}
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

      <EditProjectDialog
        open={editOpen}
        onOpenChange={setEditOpen}
        projectId={projectId}
        currentValues={{
          name: project.name,
          description: project.description,
          default_branch: project.default_branch,
        }}
      />

      <CreatePipelineDialog
        open={pipelineCreateOpen}
        onOpenChange={setPipelineCreateOpen}
        projectId={projectId}
      />

      <TriggerBuildDialog
        open={triggerBuildOpen}
        onOpenChange={(nextOpen) => {
          setTriggerBuildOpen(nextOpen)
          if (!nextOpen) {
            setTriggerPipelineId(undefined)
          }
        }}
        fixedProjectId={projectId}
        defaultPipelineId={triggerPipelineId}
        defaultBranch={project.default_branch}
        description="Run this project's pipeline now."
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
            <AlertDialogTitle>Delete project?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete "{project.name}" and all associated pipelines and builds.
              This action cannot be undone.
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
