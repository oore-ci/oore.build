import { useEffect, useState } from 'react'
import { Link, createFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
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
import { usePipelines } from '@/hooks/use-pipelines'
import {
  useDeleteProject,
  useProject,
  useUpdateProject,
} from '@/hooks/use-projects'
import { getStatusVariant } from '@/lib/status-variants'
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import PipelineCard from '@/components/pipeline-card'
import TriggerBuildDialog from '@/components/trigger-build-dialog'

const TAB_VALUES = ['pipelines', 'builds', 'settings'] as const
type TabValue = (typeof TAB_VALUES)[number]

const tabSearch = z.object({
  tab: z.enum(TAB_VALUES).optional().catch(undefined),
})

export const Route = createFileRoute('/projects/$projectId/')({
  staticData: { breadcrumbLabel: 'Details' },
  validateSearch: tabSearch,
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ProjectDetailPage,
})

/* ------------------------------------------------------------------ */
/*  Settings tab: inline project edit form                             */
/* ------------------------------------------------------------------ */

const editProjectSchema = z.object({
  name: z.string().min(1, 'Name is required'),
  description: z.string().optional(),
  default_branch: z.string().optional(),
})

type EditProjectForm = z.infer<typeof editProjectSchema>

function ProjectSettingsForm({
  projectId,
  currentValues,
}: {
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
    form.reset({
      name: currentValues.name,
      description: currentValues.description ?? '',
      default_branch: currentValues.default_branch ?? '',
    })
  }, [currentValues, form])

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
        onSuccess: () => toast.success('Project updated'),
        onError: (err) =>
          toast.error(`Failed to update project: ${err.message}`),
      },
    )
  }

  return (
    <Card>
      <CardContent>
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
            <div className="flex justify-end">
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
            </div>
          </form>
        </Form>
      </CardContent>
    </Card>
  )
}

/* ------------------------------------------------------------------ */
/*  Main page                                                          */
/* ------------------------------------------------------------------ */

function ProjectDetailPage() {
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

  const [deleteOpen, setDeleteOpen] = useState(false)
  const [dangerOpen, setDangerOpen] = useState(false)
  const [triggerBuildOpen, setTriggerBuildOpen] = useState(false)
  const [triggerPipelineId, setTriggerPipelineId] = useState<
    string | undefined
  >()

  const activeTab: TabValue = tab ?? 'pipelines'

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
    return (
      <PageLayout width="wide">
        <PageMeta title={label} noindex />
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
    setTriggerPipelineId(pipelineId)
    setTriggerBuildOpen(true)
  }

  // Build a quick lookup: pipeline.id -> last build info
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

  return (
    <PageLayout width="wide">
      <PageMeta title={label} noindex />
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
          </>
        }
        actions={
          canTriggerBuild || canDeleteProjects ? (
            <>
              {canTriggerBuild ? (
                <Button
                  onClick={() => openTriggerBuild()}
                  disabled={pipelines.length === 0 || !projectHasSource}
                >
                  <HugeiconsIcon icon={PlayIcon} size={16} />
                  Run Build
                </Button>
              ) : null}
              {canDeleteProjects ? (
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

        {/* ---- Pipelines tab ---- */}
        <TabsContent value="pipelines">
          <div className="space-y-4 pt-2">
            {canWritePipelines ? (
              <div className="flex justify-end">
                <Button
                  size="sm"
                  render={
                    <Link
                      to="/projects/$projectId/pipelines/new"
                      params={{ projectId }}
                    />
                  }
                >
                  <HugeiconsIcon icon={Add01Icon} size={14} />
                  Add Pipeline
                </Button>
              </div>
            ) : null}

            {pipelines.length === 0 ? (
              <p className="py-6 text-center text-sm text-muted-foreground">
                No pipelines yet. Add one to start building.
              </p>
            ) : (
              pipelines.map((pipeline) => {
                const lb = lastBuildByPipeline.get(pipeline.id)
                return (
                  <PipelineCard
                    key={pipeline.id}
                    pipeline={pipeline}
                    projectId={projectId}
                    defaultBranch={project.default_branch}
                    canWrite={canWritePipelines}
                    canTriggerBuild={canTriggerBuild && projectHasSource}
                    lastBuildStatus={lb?.status}
                    lastBuildTime={lb?.time}
                  />
                )
              })
            )}
          </div>
        </TabsContent>

        {/* ---- Builds tab ---- */}
        <TabsContent value="builds">
          <div className="pt-2">
            <Card>
              <CardContent>
                {builds.length === 0 ? (
                  <div className="space-y-2 py-6 text-center">
                    <p className="text-sm text-muted-foreground">
                      No builds yet.
                    </p>
                    {canTriggerBuild && pipelines.length > 0 && projectHasSource ? (
                      <Button size="sm" onClick={() => openTriggerBuild()}>
                        <HugeiconsIcon icon={PlayIcon} size={14} />
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
                        <TableHead>Commit</TableHead>
                        <TableHead>Queued</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {builds.map((build) => (
                        <TableRow
                          key={build.id}
                          className="group cursor-pointer"
                          onClick={() =>
                            void navigate({
                              to: '/builds/$buildId',
                              params: { buildId: build.id },
                            })
                          }
                        >
                          <TableCell className="font-mono text-sm group-hover:underline">
                            #{build.build_number}
                          </TableCell>
                          <TableCell>
                            <Badge variant={getStatusVariant(build.status)}>
                              {build.status}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <Badge variant="outline">
                              {build.trigger_type}
                            </Badge>
                          </TableCell>
                          <TableCell className="font-mono text-xs text-muted-foreground">
                            {build.branch ?? 'n/a'}
                          </TableCell>
                          <TableCell className="font-mono text-xs text-muted-foreground">
                            {build.commit_sha
                              ? build.commit_sha.slice(0, 10)
                              : 'n/a'}
                          </TableCell>
                          <TableCell className="text-sm text-muted-foreground">
                            {relativeTime(build.queued_at)}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                )}
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        {/* ---- Settings tab ---- */}
        <TabsContent value="settings">
          <div className="space-y-4 pt-2">
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
                          <HugeiconsIcon icon={Delete02Icon} size={16} />
                          Delete Project
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
          setTriggerBuildOpen(nextOpen)
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
