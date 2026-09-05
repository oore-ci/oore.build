import { useEffect, useMemo, useState, type ReactElement } from 'react'
import { Link } from '@tanstack/react-router'
import { useForm, useWatch } from 'react-hook-form'
import type { UseFormReturn } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import { useBuildChangelogPreview, useCreateBuild } from '@/hooks/use-builds'
import { useInfinitePipelines, usePipeline } from '@/hooks/use-pipelines'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { useRunners } from '@/hooks/use-runners'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useInfiniteProjects, useProject } from '@/hooks/use-projects'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from '@/components/ui/combobox'
import {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
  DrawerTrigger,
} from '@/components/ui/drawer'
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
import { Item, ItemContent, ItemMedia, ItemTitle } from '@/components/ui/item'
import { Spinner } from '@/components/ui/spinner'
import { Textarea } from '@/components/ui/textarea'
import type { BuildPlatform } from '@oore/client/models'
import { useAuthStore } from '@/stores/auth-store'
import { useIsMobile } from '@/hooks/use-mobile'
import { isNearScrollEnd } from '@/lib/scroll'

const platformLabels = {
  android: 'Android',
  ios: 'iOS',
  macos: 'macOS',
} satisfies Record<BuildPlatform, string>

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
        code: 'custom',
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
              <Item
                key={platform}
                render={<label />}
                variant="outline"
                size="sm"
                className="has-data-checked:border-primary has-data-checked:bg-accent"
              >
                <ItemMedia>
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
                          message:
                            'Select at least one platform for this build',
                        })
                        return
                      }
                      form.clearErrors('platforms')
                      field.onChange(next)
                    }}
                  />
                </ItemMedia>
                <ItemContent>
                  <ItemTitle>{platformLabels[platform]}</ItemTitle>
                </ItemContent>
              </Item>
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

function TriggerBuildBlockingAlerts({
  projectId,
  canConfigure,
  issues,
  onRetryPipelines,
  onRetryProjects,
}: {
  projectId?: string
  canConfigure: boolean
  issues: {
    noPipelines: boolean
    noProjects: boolean
    pipelineLoadFailed: boolean
    projectLoadFailed: boolean
    sourceMissing: boolean
  }
  onRetryPipelines: () => void
  onRetryProjects: () => void
}) {
  if (issues.projectLoadFailed) {
    return (
      <Alert variant="destructive">
        <AlertDescription className="flex items-center justify-between gap-3">
          <span>Projects could not be loaded.</span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={onRetryProjects}
          >
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    )
  }

  if (issues.pipelineLoadFailed) {
    return (
      <Alert variant="destructive">
        <AlertDescription className="flex items-center justify-between gap-3">
          <span>Pipelines could not be loaded for this project.</span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={onRetryPipelines}
          >
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    )
  }

  return (
    <>
      {issues.noProjects ? (
        <Alert variant="destructive">
          <AlertDescription>
            No projects are available for you to run. Ask a maintainer for build
            access or create a project.
          </AlertDescription>
        </Alert>
      ) : null}
      {issues.noPipelines ? (
        <Alert variant="destructive">
          <AlertDescription>
            This project has no pipelines.{' '}
            {canConfigure && projectId ? (
              <Link
                to="/projects/$projectId/pipelines/new"
                params={{ projectId }}
                className="underline underline-offset-4"
              >
                Set up a build
              </Link>
            ) : (
              'Ask a project maintainer to add one.'
            )}
          </AlertDescription>
        </Alert>
      ) : null}
      {issues.sourceMissing ? (
        <Alert variant="destructive">
          <AlertDescription>
            This project is not linked to a source repository. Ask an owner or
            admin to repair it in{' '}
            {projectId ? (
              <Link
                to="/projects/$projectId"
                params={{ projectId }}
                search={{ tab: 'settings' }}
                className="underline underline-offset-4"
              >
                project settings
              </Link>
            ) : (
              'project settings'
            )}
            .
          </AlertDescription>
        </Alert>
      ) : null}
    </>
  )
}

function TriggerBuildFooter({
  queueOnly,
  blocked,
  onSubmit,
  pending,
}: {
  queueOnly: boolean
  blocked: boolean
  onSubmit: () => void
  pending: boolean
}) {
  return (
    <DrawerFooter>
      <Button type="button" disabled={pending || blocked} onClick={onSubmit}>
        {pending ? (
          <>
            <Spinner className="size-4" />
            Running...
          </>
        ) : queueOnly ? (
          'Queue build'
        ) : (
          'Run build'
        )}
      </Button>
      <DrawerClose render={<Button variant="outline">Cancel</Button>} />
    </DrawerFooter>
  )
}

interface TriggerBuildDrawerProps {
  fixedProjectId?: string
  fixedPipelineId?: string
  fixedPipelineName?: string
  defaultPipelineId?: string
  defaultBranch?: string
  title?: string
  description?: string
  onBuildCreated?: (buildId: string) => void
  open?: boolean
  onOpenChange?: (open: boolean) => void
  children: ReactElement
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
    platforms: [],
    branch: defaultBranch ?? '',
    commit_sha: '',
    changelog: undefined,
  }
}

export default function TriggerBuildDrawer({
  fixedProjectId,
  fixedPipelineId,
  fixedPipelineName,
  defaultPipelineId,
  defaultBranch,
  title = 'Run Build',
  description = 'Queue a manual build run for a selected pipeline.',
  onBuildCreated,
  open: controlledOpen,
  onOpenChange,
  children,
}: TriggerBuildDrawerProps) {
  const isMobile = useIsMobile()
  const [internalOpen, setInternalOpen] = useState(false)
  const open = controlledOpen ?? internalOpen
  const setOpen = onOpenChange ?? setInternalOpen
  const createBuildMutation = useCreateBuild()
  const canReadSettings = useHasPermission('instance_settings:read')
  const runnersQuery = useRunners({ limit: 1 }, { enabled: open })
  const preferencesQuery = useInstancePreferences({
    enabled: open && canReadSettings,
  })
  const queueOnly =
    runnersQuery.data?.online_total === 0 ||
    preferencesQuery.data?.direct_macos_runner_paused === true
  const instanceRole = useAuthStore((state) => state.user?.role)
  const canRunEveryProject =
    instanceRole === 'owner' || instanceRole === 'admin'
  const [projectSearch, setProjectSearch] = useState('')
  const [pipelineSearch, setPipelineSearch] = useState('')
  const updateProjectSearch = useDebouncedCallback(
    (value: string) => setProjectSearch(value.trim()),
    300,
  )
  const updatePipelineSearch = useDebouncedCallback(
    (value: string) => setPipelineSearch(value.trim()),
    300,
  )
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

  const projectsQuery = useInfiniteProjects(
    {
      limit: 100,
      search: projectSearch || undefined,
      sort: 'name',
      direction: 'asc',
    },
    { enabled: open && !fixedProjectId },
  )
  const fixedProjectQuery = useProject(fixedProjectId ?? '')
  const projects = useMemo(
    () => projectsQuery.data?.pages.flatMap((page) => page.projects) ?? [],
    [projectsQuery.data?.pages],
  )
  const runnableProjects = useMemo(
    () =>
      canRunEveryProject
        ? projects
        : projects.filter((project) =>
            hasProjectPermission(project.current_user_role, 'builds:write'),
          ),
    [canRunEveryProject, projects],
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
    defaultBranch,
    defaultPipelineId,
    fixedPipelineId,
    fixedProjectId,
    form,
    open,
  ])

  useEffect(() => {
    if (
      open &&
      !fixedProjectId &&
      !form.getValues('project_id') &&
      runnableProjects[0]
    ) {
      form.setValue('project_id', runnableProjects[0].id)
    }
  }, [fixedProjectId, form, open, runnableProjects])

  const projectId = useWatch({
    control: form.control,
    name: 'project_id',
    defaultValue: fixedProjectId,
  })
  const activeProject = fixedProjectId
    ? fixedProjectQuery.data?.project
    : projects.find((project) => project.id === projectId)
  const sourceMissing =
    !!projectId &&
    !projectsQuery.isLoading &&
    !projectsQuery.error &&
    !!activeProject &&
    !activeProject.repository_id

  const pipelinesQuery = useInfinitePipelines(
    projectId ?? '',
    {
      limit: 100,
      search: pipelineSearch || undefined,
      sort: 'name',
      direction: 'asc',
    },
    { enabled: open && !!projectId },
  )
  const pipelines = useMemo(
    () => pipelinesQuery.data?.pages.flatMap((page) => page.pipelines) ?? [],
    [pipelinesQuery.data?.pages],
  )

  const selectedPipelineId = useWatch({
    control: form.control,
    name: 'pipeline_id',
    defaultValue: fixedPipelineId,
  })
  const selectedPipelineQuery = usePipeline(
    open ? (selectedPipelineId ?? '') : '',
  )
  const selectedPipeline =
    pipelines.find((pipeline) => pipeline.id === selectedPipelineId) ??
    (selectedPipelineQuery.data &&
    selectedPipelineQuery.data.pipeline.project_id === projectId
      ? selectedPipelineQuery.data.pipeline
      : undefined)
  const availablePlatforms = selectedPipeline?.execution_config.platforms ?? []
  const branchItems = useMemo(
    () =>
      Array.from(
        new Set(
          [
            defaultBranch,
            activeProject?.default_branch,
            ...(selectedPipeline?.trigger_config.branches ?? []).filter(
              (branch) =>
                !branch.includes('*') &&
                !branch.includes('?') &&
                !branch.includes('['),
            ),
          ].filter((branch): branch is string => !!branch),
        ),
      ),
    [activeProject?.default_branch, defaultBranch, selectedPipeline],
  )
  const branch = useWatch({ control: form.control, name: 'branch' })
  const commitSha = useWatch({ control: form.control, name: 'commit_sha' })
  const changelogPreviewQuery = useBuildChangelogPreview(
    projectId ?? '',
    {
      pipeline_id: selectedPipelineId ?? '',
      branch: branch?.trim(),
      commit_sha: commitSha?.trim(),
    },
    { enabled: open },
  )

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
          setOpen(false)
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
    (projectsQuery.data?.pages[0]?.total ?? 0) === 0
  const noPipelines =
    !fixedPipelineId &&
    !!projectId &&
    !pipelinesQuery.isLoading &&
    !pipelinesQuery.error &&
    pipelines.length === 0

  return (
    <Drawer
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          setProjectSearch('')
          setPipelineSearch('')
        }
        setOpen(nextOpen)
      }}
      showSwipeHandle={isMobile}
      swipeDirection={isMobile ? 'down' : 'right'}
    >
      <DrawerTrigger render={children} />
      <DrawerContent>
        <DrawerHeader>
          <DrawerTitle>{title}</DrawerTitle>
          <DrawerDescription>{description}</DrawerDescription>
        </DrawerHeader>

        <Form {...form}>
          <form
            onSubmit={form.handleSubmit(onSubmit)}
            className="flex min-h-0 flex-1 flex-col"
          >
            <div className="flex-1 scroll-fade space-y-4 overflow-y-auto p-4">
              {!fixedProjectId ? (
                <FormField
                  control={form.control}
                  name="project_id"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Project</FormLabel>
                      <Combobox
                        items={projects}
                        filter={null}
                        value={
                          projects.find(
                            (project) => project.id === field.value,
                          ) ?? null
                        }
                        onValueChange={(project) => {
                          field.onChange(project?.id ?? '')
                          form.setValue('branch', project?.default_branch ?? '')
                          form.setValue('commit_sha', '')
                          setPipelineSearch('')
                          if (!fixedPipelineId) {
                            form.setValue('pipeline_id', '', {
                              shouldDirty: true,
                            })
                            form.setValue('platforms', [], {
                              shouldDirty: false,
                            })
                          }
                        }}
                        onInputValueChange={(value, details) => {
                          if (
                            details.reason === 'input-change' ||
                            details.reason === 'input-clear'
                          ) {
                            updateProjectSearch(value)
                          }
                        }}
                        isItemEqualToValue={(item, value) =>
                          item.id === value.id
                        }
                        itemToStringLabel={(project) => project.name}
                      >
                        <FormControl>
                          <ComboboxInput
                            className="w-full"
                            disabled={projectsQuery.isLoading}
                            placeholder={
                              projectsQuery.isLoading
                                ? 'Loading projects...'
                                : 'Search projects...'
                            }
                          />
                        </FormControl>
                        <ComboboxContent>
                          <ComboboxEmpty>No matching projects.</ComboboxEmpty>
                          <ComboboxList
                            onScroll={(event) => {
                              const list = event.currentTarget
                              if (
                                isNearScrollEnd(list) &&
                                projectsQuery.hasNextPage &&
                                !projectsQuery.isFetchingNextPage
                              ) {
                                void projectsQuery.fetchNextPage()
                              }
                            }}
                          >
                            {projects.map((project) => (
                              <ComboboxItem
                                key={project.id}
                                value={project}
                                disabled={
                                  !canRunEveryProject &&
                                  !hasProjectPermission(
                                    project.current_user_role,
                                    'builds:write',
                                  )
                                }
                              >
                                {project.name}
                              </ComboboxItem>
                            ))}
                          </ComboboxList>
                        </ComboboxContent>
                      </Combobox>
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
                      <Combobox
                        items={pipelines}
                        filter={null}
                        value={selectedPipeline ?? null}
                        onValueChange={(pipeline) => {
                          field.onChange(pipeline?.id ?? '')
                          form.setValue(
                            'platforms',
                            pipeline?.execution_config.platforms ?? [],
                            { shouldDirty: false },
                          )
                        }}
                        onInputValueChange={(value, details) => {
                          if (
                            details.reason === 'input-change' ||
                            details.reason === 'input-clear'
                          ) {
                            updatePipelineSearch(value)
                          }
                        }}
                        itemToStringLabel={(pipeline) => pipeline.name}
                      >
                        <FormControl>
                          <ComboboxInput
                            className="w-full"
                            disabled={!projectId || pipelinesQuery.isLoading}
                            placeholder={
                              projectId
                                ? pipelinesQuery.isLoading
                                  ? 'Loading pipelines...'
                                  : 'Search pipelines...'
                                : 'Select a project first'
                            }
                          />
                        </FormControl>
                        <ComboboxContent>
                          <ComboboxEmpty>No matching pipelines.</ComboboxEmpty>
                          <ComboboxList
                            onScroll={(event) => {
                              const list = event.currentTarget
                              if (
                                isNearScrollEnd(list) &&
                                pipelinesQuery.hasNextPage &&
                                !pipelinesQuery.isFetchingNextPage
                              ) {
                                void pipelinesQuery.fetchNextPage()
                              }
                            }}
                          >
                            {pipelines.map((pipeline) => (
                              <ComboboxItem key={pipeline.id} value={pipeline}>
                                {pipeline.name}
                              </ComboboxItem>
                            ))}
                          </ComboboxList>
                        </ComboboxContent>
                      </Combobox>
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
                    <Combobox
                      items={branchItems}
                      inputValue={field.value ?? ''}
                      value={
                        branchItems.includes(field.value ?? '')
                          ? field.value
                          : null
                      }
                      onInputValueChange={field.onChange}
                      onValueChange={(value) => {
                        if (value) field.onChange(value)
                      }}
                    >
                      <FormControl>
                        <ComboboxInput
                          className="w-full"
                          placeholder={defaultBranch ?? 'main'}
                          autoComplete="off"
                        />
                      </FormControl>
                      <ComboboxContent>
                        <ComboboxEmpty>
                          No matching known branches. Keep typing to use a
                          custom branch.
                        </ComboboxEmpty>
                        <ComboboxList>
                          {(branch) => (
                            <ComboboxItem key={branch} value={branch}>
                              {branch}
                            </ComboboxItem>
                          )}
                        </ComboboxList>
                      </ComboboxContent>
                    </Combobox>
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
                  <FormItem className="pb-4">
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

              <TriggerBuildBlockingAlerts
                projectId={projectId}
                canConfigure={
                  !!activeProject &&
                  (canRunEveryProject ||
                    hasProjectPermission(
                      activeProject.current_user_role,
                      'pipelines:write',
                    ))
                }
                issues={{
                  noPipelines,
                  noProjects,
                  pipelineLoadFailed: pipelinesQuery.isError,
                  projectLoadFailed: projectsQuery.isError,
                  sourceMissing,
                }}
                onRetryPipelines={() => void pipelinesQuery.refetch()}
                onRetryProjects={() => void projectsQuery.refetch()}
              />
              <Alert>
                <AlertDescription>
                  {runnersQuery.error
                    ? 'Runner availability could not be checked. The build may wait for a runner.'
                    : runnersQuery.isLoading
                      ? 'Checking runner availability…'
                      : preferencesQuery.data?.direct_macos_runner_paused
                        ? 'Direct macOS builds are paused. You can queue this build; it will wait until an administrator resumes execution.'
                        : runnersQuery.data?.online_total === 0
                          ? 'No runner is online. You can queue this build; it will wait for a runner to connect.'
                          : 'A runner is online. Toolchain, repository access and signing are checked when the build runs.'}{' '}
                  <Link
                    to="/settings/runners"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="underline underline-offset-4"
                  >
                    Review runners in a new tab
                  </Link>
                  .
                  {canReadSettings &&
                  preferencesQuery.data?.direct_macos_runner_paused ? (
                    <>
                      {' '}
                      <Link
                        to="/settings/preferences"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="underline underline-offset-4"
                      >
                        Review execution settings in a new tab
                      </Link>
                      .
                    </>
                  ) : null}
                </AlertDescription>
              </Alert>
            </div>

            <TriggerBuildFooter
              queueOnly={queueOnly}
              blocked={
                noProjects ||
                noPipelines ||
                sourceMissing ||
                projectsQuery.isError ||
                pipelinesQuery.isError ||
                projectsQuery.isLoading ||
                pipelinesQuery.isLoading ||
                (!fixedProjectId && !projectId)
              }
              onSubmit={() => void form.handleSubmit(onSubmit)()}
              pending={createBuildMutation.isPending}
            />
          </form>
        </Form>
      </DrawerContent>
    </Drawer>
  )
}
