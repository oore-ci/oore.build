import { useMemo } from 'react'
import { useForm } from 'react-hook-form'
import type { UseFormReturn } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'
import { useMountEffect } from '@/hooks/use-mount-effect'

import { useBuildChangelogPreview, useCreateBuild } from '@/hooks/use-builds'
import { usePipelines } from '@/hooks/use-pipelines'
import { hasProjectPermission } from '@/hooks/use-permissions'
import { useProjects } from '@/hooks/use-projects'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
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
  FormDescription,
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
import { Textarea } from '@/components/ui/textarea'
import type { BuildPlatform } from '@/lib/types'

const platformLabels: Record<BuildPlatform, string> = {
  android: 'Android',
  ios: 'iOS',
  macos: 'macOS',
}

const triggerBuildSchema = z
  .object({
    project_id: z.string().optional(),
    pipeline_id: z.string().optional(),
    platforms: z.array(z.enum(['android', 'ios', 'macos'])),
    branch: z.string().optional(),
    commit_sha: z.string().optional(),
    changelog: z
      .string()
      .max(4000, 'Keep the changelog under 4,000 characters')
      .optional(),
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

function PlatformSelectionField({
  form,
  platforms,
}: {
  form: UseFormReturn<TriggerBuildForm>
  platforms: Array<BuildPlatform>
}) {
  if (platforms.length < 2) return null

  return (
    <FormField
      control={form.control}
      name="platforms"
      render={({ field }) => (
        <FormItem>
          <FormLabel>Platforms for this run</FormLabel>
          <div className="grid gap-2 sm:grid-cols-3">
            {platforms.map((platform) => (
              <label
                key={platform}
                className="flex items-center gap-2 border border-border px-3 py-2 text-sm"
              >
                <Checkbox
                  checked={
                    field.value.length === 0 || field.value.includes(platform)
                  }
                  onCheckedChange={(checked) => {
                    const current =
                      field.value.length === 0 ? platforms : field.value
                    const next = checked
                      ? [...current, platform].filter(
                          (value, index, values) =>
                            values.indexOf(value) === index,
                        )
                      : current.filter((value) => value !== platform)
                    if (next.length === 0) {
                      form.setError('platforms', {
                        message: 'Select at least one platform for this build',
                      })
                      return
                    }
                    form.clearErrors('platforms')
                    field.onChange(next)
                  }}
                />
                {platformLabels[platform]}
              </label>
            ))}
          </div>
          <FormDescription>
            Applies to this build only. Automatic builds still run every
            configured platform.
          </FormDescription>
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

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
  firstProjectId?: string,
  platforms: Array<BuildPlatform> = [],
): TriggerBuildForm {
  return {
    project_id: fixedProjectId ?? firstProjectId ?? '',
    pipeline_id: fixedPipelineId ?? defaultPipelineId ?? '',
    platforms,
    branch: defaultBranch ?? '',
    commit_sha: '',
    changelog: undefined,
  }
}

function useTriggerBuildDialogState({
  open,
  onOpenChange,
  fixedProjectId,
  fixedPipelineId,
  fixedPipelineName,
  defaultPipelineId,
  defaultBranch,
  title = 'Run Build',
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

  const projectsQuery = useProjects({ limit: 200 }, { enabled: open })
  const projects = useMemo(
    () =>
      (projectsQuery.data?.projects ?? []).filter((project) =>
        hasProjectPermission(project.current_user_role, 'builds', 'write'),
      ),
    [projectsQuery.data?.projects],
  )

  const projectId = fixedProjectId ?? form.watch('project_id') ?? ''
  const activeProject = useMemo(
    () => projects.find((project) => project.id === projectId),
    [projects, projectId],
  )
  const sourceMissing =
    !!projectId &&
    !projectsQuery.isLoading &&
    !projectsQuery.error &&
    !!activeProject &&
    !activeProject.repository_id

  const pipelinesQuery = usePipelines(
    projectId,
    { limit: 200 },
    { enabled: open && !!projectId },
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
  const selectedPipelineId = fixedPipelineId ?? form.watch('pipeline_id') ?? ''
  const selectedPipeline = useMemo(
    () => pipelines.find((pipeline) => pipeline.id === selectedPipelineId),
    [pipelines, selectedPipelineId],
  )
  const availablePlatforms = useMemo(
    () => selectedPipeline?.execution_config.platforms ?? [],
    [selectedPipeline],
  )
  const changelogPreviewQuery = useBuildChangelogPreview(
    projectId,
    {
      pipeline_id: selectedPipelineId,
      branch: form.watch('branch')?.trim() || undefined,
      commit_sha: form.watch('commit_sha')?.trim() || undefined,
    },
    { enabled: open },
  )

  // Auto-select pipeline when project changes
  useMountEffect(() => {
    const subscription = form.watch((_, { name }) => {
      if (name !== 'project_id') return
      if (fixedPipelineId) return
      form.setValue('pipeline_id', '', { shouldDirty: false })
      form.setValue('platforms', [], { shouldDirty: false })
    })
    return () => subscription.unsubscribe()
  })

  function handleClose() {
    onOpenChange(false)
  }

  function onSubmit(data: TriggerBuildForm) {
    const resolvedProjectId = fixedProjectId ?? data.project_id?.trim() ?? ''
    if (!resolvedProjectId) {
      form.setError('project_id', { message: 'Project is required' })
      return
    }
    if (sourceMissing) {
      toast.error(
        'Project source is not linked. Connect and link a repository before triggering builds.',
      )
      return
    }

    const resolvedPipelineId = fixedPipelineId ?? data.pipeline_id?.trim() ?? ''
    if (!resolvedPipelineId) {
      form.setError('pipeline_id', { message: 'Pipeline is required' })
      return
    }
    const branch = data.branch?.trim() || undefined
    const commitSha =
      data.commit_sha?.trim() ||
      changelogPreviewQuery.data?.target_commit ||
      undefined
    const changelog =
      (data.changelog === undefined
        ? changelogPreviewQuery.data?.markdown
        : data.changelog
      )?.trim() || undefined

    createBuildMutation.mutate(
      {
        projectId: resolvedProjectId,
        data: {
          pipeline_id: resolvedPipelineId,
          branch,
          commit_sha: commitSha,
          trigger_ref: branch,
          changelog,
          platforms:
            availablePlatforms.length > 1
              ? data.platforms.length > 0
                ? data.platforms
                : availablePlatforms
              : undefined,
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

  return {
    createBuildMutation,
    changelogPreviewQuery,
    defaultBranch,
    defaultPipelineId,
    description,
    fixedPipelineId,
    fixedPipelineName,
    fixedProjectId,
    form,
    handleClose,
    noPipelines,
    noProjects,
    onOpenChange,
    onSubmit,
    open,
    pipelineItems,
    pipelines,
    pipelinesQuery,
    availablePlatforms,
    projectId,
    projectItems,
    projects,
    projectsQuery,
    sourceMissing,
    title,
  }
}

export default function TriggerBuildDialog(props: TriggerBuildDialogProps) {
  const {
    createBuildMutation,
    changelogPreviewQuery,
    defaultBranch,
    defaultPipelineId,
    description,
    fixedPipelineId,
    fixedPipelineName,
    fixedProjectId,
    form,
    handleClose,
    noPipelines,
    noProjects,
    onOpenChange,
    onSubmit,
    open,
    pipelineItems,
    pipelines,
    pipelinesQuery,
    availablePlatforms,
    projectId,
    projectItems,
    projects,
    projectsQuery,
    sourceMissing,
    title,
  } = useTriggerBuildDialogState(props)

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          form.reset(
            defaults(
              fixedProjectId,
              fixedPipelineId,
              defaultPipelineId,
              defaultBranch,
              !fixedProjectId ? projects[0]?.id : undefined,
              availablePlatforms,
            ),
          )
        }
        onOpenChange(nextOpen)
      }}
    >
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
                      disabled={projectsQuery.isLoading}
                      items={projectItems}
                    >
                      <FormControl>
                        <SelectTrigger className="w-full">
                          <SelectValue
                            placeholder={
                              projectsQuery.isLoading
                                ? 'Loading projects...'
                                : 'Select a project'
                            }
                          />
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
                      onValueChange={(value) => {
                        field.onChange(value)
                        const pipeline = pipelines.find(
                          (candidate) => candidate.id === value,
                        )
                        form.setValue(
                          'platforms',
                          pipeline?.execution_config.platforms ?? [],
                          { shouldDirty: false },
                        )
                      }}
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

            <PlatformSelectionField
              form={form}
              platforms={availablePlatforms}
            />

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
                  <FormDescription>
                    The branch to build. If both branch and commit SHA are
                    provided, the commit takes precedence.
                  </FormDescription>
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

            <FormField
              control={form.control}
              name="changelog"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>What changed? (optional)</FormLabel>
                  <FormControl>
                    <Textarea
                      {...field}
                      value={
                        field.value ??
                        changelogPreviewQuery.data?.markdown ??
                        ''
                      }
                      placeholder="No changes found since the previous build."
                      rows={5}
                    />
                  </FormControl>
                  <FormDescription>
                    {changelogPreviewQuery.isFetching
                      ? 'Drafting from commits since the previous successful build…'
                      : changelogPreviewQuery.error
                        ? 'Could not generate a draft. You can still write one.'
                        : 'Markdown draft generated from commit titles and authors. Edit or clear it before running.'}
                  </FormDescription>
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
            {sourceMissing ? (
              <Alert variant="destructive">
                <AlertDescription>
                  This project is not linked to a source repository. Link a
                  repository before triggering builds.
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
                  sourceMissing ||
                  (!fixedProjectId && !projectId)
                }
                onClick={() => {
                  void form.handleSubmit(onSubmit)()
                }}
              >
                {createBuildMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Running...
                  </>
                ) : (
                  'Run build'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
