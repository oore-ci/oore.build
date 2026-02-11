import { useEffect, useMemo } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { z } from 'zod'
import { toast } from 'sonner'

import { useCreateBuild } from '@/hooks/use-builds'
import { usePipelines } from '@/hooks/use-pipelines'
import { useProjects } from '@/hooks/use-projects'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'

const triggerBuildSchema = z
  .object({
    project_id: z.string().optional(),
    pipeline_id: z.string().optional(),
    branch: z.string().optional(),
    commit_sha: z.string().optional(),
  })
  .superRefine((data, ctx) => {
    const branch = data.branch?.trim()
    const commit = data.commit_sha?.trim()
    if (!branch && !commit) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Provide a branch or a commit SHA',
        path: ['branch'],
      })
    }
  })

type TriggerBuildForm = z.infer<typeof triggerBuildSchema>

interface TriggerBuildDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  fixedProjectId?: string
  fixedPipelineId?: string
  fixedPipelineName?: string
  defaultPipelineId?: string
  defaultBranch?: string
  title?: string
  description?: string
  onBuildCreated?: (buildId: string) => void
}

function defaults(
  fixedProjectId?: string,
  fixedPipelineId?: string,
  defaultPipelineId?: string,
  defaultBranch?: string,
): TriggerBuildForm {
  return {
    project_id: fixedProjectId ?? '',
    pipeline_id: fixedPipelineId ?? defaultPipelineId ?? '',
    branch: defaultBranch ?? '',
    commit_sha: '',
  }
}

export default function TriggerBuildDialog({
  open,
  onOpenChange,
  fixedProjectId,
  fixedPipelineId,
  fixedPipelineName,
  defaultPipelineId,
  defaultBranch,
  title = 'Trigger Build',
  description = 'Queue a manual build run for a selected pipeline.',
  onBuildCreated,
}: TriggerBuildDialogProps) {
  const createBuildMutation = useCreateBuild()
  const form = useForm<TriggerBuildForm>({
    resolver: zodResolver(triggerBuildSchema),
    defaultValues: defaults(
      fixedProjectId,
      fixedPipelineId,
      defaultPipelineId,
      defaultBranch,
    ),
    mode: 'onBlur',
    shouldUnregister: false,
  })

  const projectsQuery = useProjects(
    { limit: 200 },
    { enabled: open && !fixedProjectId },
  )
  const projects = useMemo(
    () => projectsQuery.data?.projects ?? [],
    [projectsQuery.data?.projects],
  )

  const projectId = fixedProjectId ?? form.watch('project_id') ?? ''

  const pipelinesQuery = usePipelines(
    projectId,
    { limit: 200 },
    { enabled: open && !fixedPipelineId && !!projectId },
  )
  const pipelines = useMemo(
    () => pipelinesQuery.data?.pipelines ?? [],
    [pipelinesQuery.data?.pipelines],
  )

  const projectItems = useMemo(
    () => Object.fromEntries(projects.map((p) => [p.id, p.name])),
    [projects],
  )
  const pipelineItems = useMemo(
    () => Object.fromEntries(pipelines.map((p) => [p.id, p.name])),
    [pipelines],
  )

  useEffect(() => {
    if (!open) return
    form.reset(
      defaults(
        fixedProjectId,
        fixedPipelineId,
        defaultPipelineId,
        defaultBranch,
      ),
    )
  }, [
    open,
    fixedProjectId,
    fixedPipelineId,
    defaultPipelineId,
    defaultBranch,
    form,
  ])

  useEffect(() => {
    if (!open || fixedProjectId) return
    if (projects.length === 0) return

    const current = form.getValues('project_id')?.trim()
    if (!current) {
      form.setValue('project_id', projects[0].id, { shouldDirty: false })
    }
  }, [open, fixedProjectId, projects, form])

  useEffect(() => {
    if (!open || fixedPipelineId) return
    if (!projectId) return

    const current = form.getValues('pipeline_id')?.trim()
    const currentIsValid = pipelines.some((pipeline) => pipeline.id === current)

    if (
      defaultPipelineId &&
      pipelines.some((pipeline) => pipeline.id === defaultPipelineId)
    ) {
      if (!current || !currentIsValid) {
        form.setValue('pipeline_id', defaultPipelineId, { shouldDirty: false })
      }
      return
    }

    if (!currentIsValid) {
      form.setValue('pipeline_id', pipelines[0]?.id ?? '', {
        shouldDirty: false,
      })
    }
  }, [open, fixedPipelineId, projectId, pipelines, defaultPipelineId, form])

  function handleClose() {
    onOpenChange(false)
  }

  function onSubmit(data: TriggerBuildForm) {
    const resolvedProjectId = fixedProjectId ?? data.project_id?.trim() ?? ''
    if (!resolvedProjectId) {
      form.setError('project_id', { message: 'Project is required' })
      return
    }

    const resolvedPipelineId = fixedPipelineId ?? data.pipeline_id?.trim() ?? ''
    if (!resolvedPipelineId) {
      form.setError('pipeline_id', { message: 'Pipeline is required' })
      return
    }

    const branch = data.branch?.trim() || undefined
    const commitSha = data.commit_sha?.trim() || undefined

    createBuildMutation.mutate(
      {
        projectId: resolvedProjectId,
        data: {
          pipeline_id: resolvedPipelineId,
          branch,
          commit_sha: commitSha,
          trigger_ref: branch,
        },
      },
      {
        onSuccess: (result) => {
          toast.success(`Build #${result.build.build_number} queued`)
          onOpenChange(false)
          onBuildCreated?.(result.build.id)
        },
        onError: (error) => {
          toast.error(`Failed to trigger build: ${error.message}`)
        },
      },
    )
  }

  const noProjects =
    !fixedProjectId &&
    !projectsQuery.isLoading &&
    !projectsQuery.error &&
    projects.length === 0
  const noPipelines =
    !fixedPipelineId &&
    !!projectId &&
    !pipelinesQuery.isLoading &&
    !pipelinesQuery.error &&
    pipelines.length === 0

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            {!fixedProjectId ? (
              <FormField
                control={form.control}
                name="project_id"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Project</FormLabel>
                    <Select
                      value={field.value ?? ''}
                      onValueChange={(value) => {
                        field.onChange(value)
                        if (!fixedPipelineId) {
                          form.setValue('pipeline_id', '', {
                            shouldDirty: true,
                          })
                        }
                      }}
                      items={projectItems}
                    >
                      <FormControl>
                        <SelectTrigger className="w-full">
                          <SelectValue placeholder="Select a project" />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent>
                        {projects.map((project) => (
                          <SelectItem key={project.id} value={project.id}>
                            {project.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <FormMessage />
                  </FormItem>
                )}
              />
            ) : null}

            {!fixedPipelineId ? (
              <FormField
                control={form.control}
                name="pipeline_id"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Pipeline</FormLabel>
                    <Select
                      value={field.value ?? ''}
                      onValueChange={field.onChange}
                      disabled={!projectId || pipelinesQuery.isLoading}
                      items={pipelineItems}
                    >
                      <FormControl>
                        <SelectTrigger className="w-full">
                          <SelectValue
                            placeholder={
                              projectId
                                ? pipelinesQuery.isLoading
                                  ? 'Loading pipelines...'
                                  : 'Select a pipeline'
                                : 'Select a project first'
                            }
                          />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent>
                        {pipelines.map((pipeline) => (
                          <SelectItem key={pipeline.id} value={pipeline.id}>
                            {pipeline.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <FormMessage />
                  </FormItem>
                )}
              />
            ) : (
              <div className="space-y-1">
                <p className="text-sm font-medium">Pipeline</p>
                <p className="text-sm text-muted-foreground">
                  {fixedPipelineName ?? fixedPipelineId}
                </p>
              </div>
            )}

            <FormField
              control={form.control}
              name="branch"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Branch</FormLabel>
                  <FormControl>
                    <Input
                      placeholder={defaultBranch ?? 'main'}
                      autoComplete="off"
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="commit_sha"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Commit SHA (optional)</FormLabel>
                  <FormControl>
                    <Input
                      placeholder="e.g. a1b2c3d4..."
                      autoComplete="off"
                      className="font-mono"
                      {...field}
                    />
                  </FormControl>
                  <p className="text-xs text-muted-foreground">
                    If set, the runner checks out this exact commit.
                  </p>
                  <FormMessage />
                </FormItem>
              )}
            />

            {noProjects ? (
              <Alert variant="destructive">
                <AlertDescription>
                  No projects available. Create a project first.
                </AlertDescription>
              </Alert>
            ) : null}

            {noPipelines ? (
              <Alert variant="destructive">
                <AlertDescription>
                  This project has no pipelines. Add one before triggering
                  builds.
                </AlertDescription>
              </Alert>
            ) : null}

            <DialogFooter>
              <Button type="button" variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button
                type="button"
                disabled={
                  createBuildMutation.isPending ||
                  noProjects ||
                  noPipelines ||
                  (!fixedProjectId && !projectId)
                }
                onClick={() => {
                  void form.handleSubmit(onSubmit)()
                }}
              >
                {createBuildMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Triggering...
                  </>
                ) : (
                  'Trigger Build'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
