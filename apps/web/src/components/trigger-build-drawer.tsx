import { useEffect, useMemo, useState, type ReactElement } from 'react'
import { Link } from '@tanstack/react-router'
import { useForm, useWatch } from 'react-hook-form'
import type { UseFormReturn } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { toast } from '@/lib/toast'
import { useBuildChangelogPreview, useCreateBuild } from '@/hooks/use-builds'
import {
  useInfinitePipelines,
  usePipeline,
  usePipelineWorkflows,
} from '@/hooks/use-pipelines'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { useRunners } from '@/hooks/use-runners'
import { useInstancePreferences } from '@/hooks/use-artifact-storage'
import { useInfiniteProjects, useProject } from '@/hooks/use-projects'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { useDebouncedJson } from '@/hooks/use-debounced-json'
import { isOoreApiError } from '@oore/client/client'
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
import {
  useRecentRuns,
  useRecentRunsScope,
  useRecentRunsStore,
  type RecentRun,
} from '@/stores/recent-runs-store'
import { Badge } from '@/components/ui/badge'
import { useIsMobile } from '@/hooks/use-mobile'
import { isNearScrollEnd } from '@/lib/scroll'
import { Stack } from '@/components/layout/stack'
import { Row } from '@/components/layout/row'
import { Text } from '@/components/layout/text'
import { TriggerBuildStoreSection } from '@/components/trigger-build-store-section'
import { TriggerBuildWorkflowField } from '@/components/trigger-build-workflow-field'
import {
  choosableWorkflows,
  defaultWorkflowChoice,
} from '@/lib/pipeline-form-utils'
import {
  triggerBuildSchema,
  type TriggerBuildForm,
} from '@/components/trigger-build-form'

/** Typing a branch or commit settles for this long before workflows load. */
const WORKFLOW_LOOKUP_DEBOUNCE_MS = 400

const platformLabels = {
  android: 'Android',
  ios: 'iOS',
  macos: 'macOS',
} satisfies Record<BuildPlatform, string>

/** The same optional marker as the New project drawer. */
function OptionalLabel({ children }: { children: string }) {
  return (
    <FormLabel>
      {children}{' '}
      <span className="font-normal text-muted-foreground">(optional)</span>
    </FormLabel>
  )
}

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
          <FormLabel>Platforms</FormLabel>
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
                          message: 'Select at least one platform',
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

/**
 * The platforms to send: the checked ones that the pipeline or workflow
 * builds, or all of them. A single platform needs no choice.
 */
function buildPlatforms(
  available: Array<BuildPlatform>,
  checked: Array<BuildPlatform>,
): Array<BuildPlatform> | undefined {
  if (available.length < 2) return undefined
  const picked = checked.filter((platform) => available.includes(platform))
  return picked.length > 0 ? picked : available
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
          <span>Apps could not be loaded.</span>
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
          <span>Pipelines could not be loaded for this app.</span>
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
            No apps are available for you to run. Ask a maintainer for build
            access or create an app.
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
              'Ask an app maintainer to add one.'
            )}
          </AlertDescription>
        </Alert>
      ) : null}
      {issues.sourceMissing ? (
        <Alert variant="destructive">
          <AlertDescription>
            This app is not linked to a source repository. Ask an owner or admin
            to repair it in{' '}
            {projectId ? (
              <Link
                to="/projects/$projectId"
                params={{ projectId }}
                search={{ tab: 'settings' }}
                className="underline underline-offset-4"
              >
                app settings
              </Link>
            ) : (
              'app settings'
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
      <DrawerClose render={<Button variant="outline">Cancel</Button>} />
      <Button type="button" disabled={pending || blocked} onClick={onSubmit}>
        {pending ? (
          <>
            <Spinner className="size-4" />
            Queuing...
          </>
        ) : queueOnly ? (
          'Queue build'
        ) : (
          'Run build'
        )}
      </Button>
    </DrawerFooter>
  )
}

/** Drafted notes longer than this show as a short preview until edited. */
const CHANGELOG_PREVIEW_LINES = 3

/** The first few lines of drafted notes, and how many more there are. */
export function changelogPreview(markdown: string) {
  const all = markdown
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
  return {
    lines: all.slice(0, CHANGELOG_PREVIEW_LINES),
    more: Math.max(0, all.length - CHANGELOG_PREVIEW_LINES),
  }
}

/** The newest commit title in drafted notes, which list newest first. */
export function latestCommitTitle(markdown: string | undefined) {
  const first = markdown?.split('\n')[0]?.trim()
  if (!first?.startsWith('- ') || first.startsWith('- …')) return undefined
  return first
    .slice(2)
    .replace(/ — [^—]*$/, '')
    .replace(/\\(.)/g, '$1')
}

function DraftedChangelog({
  markdown,
  onEdit,
  onClear,
}: {
  markdown: string
  onEdit: () => void
  onClear: () => void
}) {
  const { lines, more } = changelogPreview(markdown)
  return (
    <Stack gap={2} className="rounded-lg border p-3">
      <ul className="space-y-1 text-sm">
        {lines.map((line) => (
          <li key={line} className="truncate">
            {line.replace(/^- /, '').replace(/\\(.)/g, '$1')}
          </li>
        ))}
      </ul>
      <Row gap={2} className="flex-wrap justify-between">
        <Text variant="caption">
          {more > 0
            ? `and ${more} more`
            : 'From the commits since the last build'}
        </Text>
        <Row gap={1}>
          <Button type="button" variant="ghost" size="sm" onClick={onClear}>
            Clear
          </Button>
          <Button type="button" variant="outline" size="sm" onClick={onEdit}>
            Edit
          </Button>
        </Row>
      </Row>
    </Stack>
  )
}

interface TriggerBuildDrawerProps {
  fixedProjectId?: string
  fixedPipelineId?: string
  fixedPipelineName?: string
  defaultPipelineId?: string
  defaultBranch?: string
  /**
   * Values to start from, such as a build being re-run with changes. Pass a
   * stable object: a new one resets the form.
   */
  prefill?: Partial<TriggerBuildForm>
  title?: string
  description?: string
  onBuildCreated?: (buildId: string) => void
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /** Omit when something else (such as Quick open) controls `open`. */
  children?: ReactElement
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
    workflow: '',
    changelog: undefined,
    store_notes: '',
  }
}

export default function TriggerBuildDrawer({
  fixedProjectId,
  fixedPipelineId,
  fixedPipelineName,
  defaultPipelineId,
  defaultBranch,
  prefill,
  title = 'Run build',
  description = 'Choose a pipeline and branch to build.',
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
  const [resolvingHead, setResolvingHead] = useState(false)
  // A signing refusal for a build with no workflow field to show it on.
  const [signingError, setSigningError] = useState<string>()
  const [pipelineSearch, setPipelineSearch] = useState('')
  const [showCommit, setShowCommit] = useState(false)
  const [recentRuns, rememberRun] = useRecentRuns()
  const recentRunsScope = useRecentRunsScope()
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

    const initial = {
      ...defaults(
        fixedProjectId,
        fixedPipelineId,
        defaultPipelineId,
        defaultBranch,
      ),
      ...prefill,
    }
    // Start from the last pipeline and branch this person ran here, unless
    // the page opening the drawer already chose them.
    const last =
      defaultPipelineId || defaultBranch || prefill
        ? undefined
        : // Read once as the drawer opens, so a run started now does not
          // reset the form.
          (useRecentRunsStore.getState().runs[recentRunsScope] ?? []).find(
            (run) =>
              (!fixedProjectId || run.projectId === fixedProjectId) &&
              (!fixedPipelineId || run.pipelineId === fixedPipelineId),
          )
    form.reset(
      last
        ? {
            ...initial,
            project_id: last.projectId,
            pipeline_id: last.pipelineId,
            branch: last.branch,
          }
        : initial,
    )
  }, [
    defaultBranch,
    defaultPipelineId,
    fixedPipelineId,
    fixedProjectId,
    form,
    open,
    prefill,
    recentRunsScope,
  ])

  // Only preselect when there is nothing to choose between; otherwise the
  // first project alphabetically would be queued by accident.
  useEffect(() => {
    const onlyProject =
      runnableProjects.length === 1 ? runnableProjects[0] : undefined
    if (
      open &&
      !fixedProjectId &&
      !form.getValues('project_id') &&
      onlyProject
    ) {
      form.setValue('project_id', onlyProject.id)
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
  // Workflows are read from the commit when one is typed, else the branch.
  const wantedReference = commitSha?.trim() || branch?.trim() || ''
  const [workflowReference, setWorkflowReference] = useState(wantedReference)
  useDebouncedJson(
    wantedReference,
    WORKFLOW_LOOKUP_DEBOUNCE_MS,
    setWorkflowReference,
  )
  const workflowsQuery = usePipelineWorkflows(
    selectedPipelineId ?? '',
    { ref: workflowReference || undefined },
    { enabled: open && !!selectedPipelineId },
  )
  const workflowChoices = choosableWorkflows(workflowsQuery.data)
  const workflowName = useWatch({ control: form.control, name: 'workflow' })
  const chosenWorkflow = workflowChoices.find(
    (workflow) => workflow.name === workflowName,
  )
  // A picked workflow decides the platforms; otherwise the pipeline does.
  const availablePlatforms =
    chosenWorkflow && chosenWorkflow.platforms.length > 0
      ? chosenWorkflow.platforms
      : (selectedPipeline?.execution_config.platforms ?? [])

  // Keep the pick when the new branch still defines it; otherwise fall back
  // to the pipeline's default or the file's only workflow.
  const workflowsData = workflowsQuery.data
  useEffect(() => {
    if (!open || !workflowsData) return
    const current = form.getValues('workflow') ?? ''
    const names = choosableWorkflows(workflowsData).map((w) => w.name)
    if (current && names.includes(current)) return
    const next = defaultWorkflowChoice(workflowsData)
    if (next === current) return
    form.setValue('workflow', next, { shouldDirty: false })
    // Keep a preset pick (Resubmit picks the rejecting store's platform)
    // when the new workflow still builds it; buildPlatforms() drops the rest.
    const builds = choosableWorkflows(workflowsData).find(
      (workflow) => workflow.name === next,
    )?.platforms
    const kept = (form.getValues('platforms') ?? []).filter(
      (platform) => !builds?.length || builds.includes(platform),
    )
    form.setValue('platforms', kept, { shouldDirty: false })
  }, [form, open, workflowsData])
  const changelogPreviewQuery = useBuildChangelogPreview(
    projectId ?? '',
    {
      pipeline_id: selectedPipelineId ?? '',
      branch: branch?.trim() || undefined,
      commit_sha: commitSha?.trim() || undefined,
    },
    { enabled: open },
  )

  async function onSubmit(data: TriggerBuildForm) {
    setSigningError(undefined)
    const resolvedProjectId = fixedProjectId ?? data.project_id?.trim() ?? ''
    if (!resolvedProjectId) {
      form.setError('project_id', { message: 'App is required' })
      return
    }
    if (sourceMissing) {
      toast.error('Link a repository in app settings before running a build.')
      return
    }

    const resolvedPipelineId = fixedPipelineId ?? data.pipeline_id?.trim() ?? ''
    if (!resolvedPipelineId) {
      form.setError('pipeline_id', { message: 'Pipeline is required' })
      return
    }
    const branch = data.branch?.trim() || undefined
    const typedCommit = data.commit_sha?.trim() || undefined
    // The drafted changelog and its commit can be minutes old. Read the
    // branch head again so the build and its changelog match what is pushed
    // now; if that fails, let the daemon resolve the head itself.
    let preview = changelogPreviewQuery.data
    if (!typedCommit && branch) {
      setResolvingHead(true)
      const refreshed = await changelogPreviewQuery.refetch()
      setResolvingHead(false)
      preview = refreshed.isSuccess ? refreshed.data : undefined
    }
    // A file with several workflows and no default needs a pick; without
    // one the daemon refuses the build.
    const workflow =
      workflowChoices.length > 0
        ? data.workflow?.trim() || undefined
        : undefined
    if (workflowChoices.length > 1 && !workflow) {
      form.setError('workflow', { message: 'Choose a workflow to build' })
      return
    }
    const commitSha = typedCommit ?? preview?.target_commit
    const changelog =
      (data.changelog === undefined
        ? preview?.markdown
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
          store_notes: data.store_notes?.trim() || undefined,
          workflow,
          platforms: buildPlatforms(availablePlatforms, data.platforms),
        },
      },
      {
        onSuccess: (result) => {
          const pipelineName =
            fixedPipelineName ??
            pipelines.find((pipeline) => pipeline.id === resolvedPipelineId)
              ?.name
          if (branch && pipelineName) {
            rememberRun({
              projectId: resolvedProjectId,
              projectName: activeProject?.name ?? '',
              pipelineId: resolvedPipelineId,
              pipelineName,
              branch,
            })
          }
          toast.success(`Build #${result.build.build_number} queued`)
          setOpen(false)
          onBuildCreated?.(result.build.id)
        },
        onError: (error) => {
          if (isOoreApiError(error) && error.code === 'invalid_workflow') {
            // The file changed or the pick is stale: show why on the field
            // and read the workflows again.
            void workflowsQuery.refetch()
            if (workflowChoices.length > 0) {
              form.setError('workflow', { message: error.message })
            } else {
              toast.error(`Choose a workflow to run: ${error.message}`)
            }
            return
          }
          if (isOoreApiError(error) && error.code === 'invalid_signing') {
            // The workflow names a signing identity the project's library
            // cannot satisfy: show why where the workflow is chosen.
            if (workflowChoices.length > 0) {
              form.setError('workflow', { message: error.message })
            } else {
              setSigningError(error.message)
            }
            return
          }
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
          setSigningError(undefined)
          setShowCommit(false)
        }
        setOpen(nextOpen)
      }}
      showSwipeHandle={isMobile}
      swipeDirection={isMobile ? 'down' : 'right'}
    >
      {children ? <DrawerTrigger render={children} /> : null}
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
              <RecentRunChips
                runs={recentRuns.filter(
                  (run) =>
                    (!fixedProjectId || run.projectId === fixedProjectId) &&
                    (!fixedPipelineId || run.pipelineId === fixedPipelineId),
                )}
                showProject={!fixedProjectId}
                showPipeline={!fixedPipelineId}
                onPick={(run) => {
                  form.setValue('project_id', run.projectId)
                  form.setValue('pipeline_id', run.pipelineId)
                  form.setValue('branch', run.branch)
                  form.setValue('commit_sha', '')
                  form.setValue('platforms', [], { shouldDirty: false })
                  form.setValue('changelog', undefined)
                  setShowCommit(false)
                }}
              />
              {!fixedProjectId ? (
                <FormField
                  control={form.control}
                  name="project_id"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>App</FormLabel>
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
                                ? 'Loading apps...'
                                : 'Search apps...'
                            }
                          />
                        </FormControl>
                        <ComboboxContent>
                          <ComboboxEmpty>No matching apps.</ComboboxEmpty>
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
                                : 'Select an app first'
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
                  <Text variant="muted">
                    {fixedPipelineName ?? fixedPipelineId}
                  </Text>
                </div>
              )}

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
                      onInputValueChange={(value, details) => {
                        // Closing without a pick resets the input to the
                        // selected item, which a typed branch is not. That
                        // reset is not a user edit; keep what was typed.
                        if (details.event.type === 'base-ui') return
                        field.onChange(value)
                      }}
                      onValueChange={(value) => {
                        if (value) field.onChange(value)
                      }}
                    >
                      <FormControl>
                        <ComboboxInput
                          className="w-full"
                          disabled={!projectId}
                          placeholder={
                            projectId
                              ? (activeProject?.default_branch ??
                                defaultBranch ??
                                'Enter a branch')
                              : 'Select an app first'
                          }
                          autoComplete="off"
                        />
                      </FormControl>
                      <ComboboxContent>
                        <ComboboxEmpty>
                          No suggestions. Enter a branch name to use it.
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
                    <BranchHeadDescription
                      title={
                        commitSha?.trim()
                          ? undefined
                          : latestCommitTitle(
                              changelogPreviewQuery.data?.markdown,
                            )
                      }
                      commit={changelogPreviewQuery.data?.target_commit}
                    />
                    {!showCommit ? (
                      <Button
                        type="button"
                        variant="link"
                        size="sm"
                        className="h-auto self-start px-0"
                        disabled={!projectId}
                        onClick={() => setShowCommit(true)}
                      >
                        Build a specific commit
                      </Button>
                    ) : null}
                    <FormMessage />
                  </FormItem>
                )}
              />

              {selectedPipelineId ? (
                <TriggerBuildWorkflowField
                  form={form}
                  data={workflowsQuery.data}
                  pending={workflowsQuery.isPending}
                  error={workflowsQuery.error}
                  reference={
                    workflowsQuery.data?.reference ?? workflowReference
                  }
                />
              ) : null}

              <PlatformSelectionField
                form={form}
                platforms={availablePlatforms}
              />

              {showCommit ? (
                <FormField
                  control={form.control}
                  name="commit_sha"
                  render={({ field }) => (
                    <FormItem>
                      <OptionalLabel>Commit SHA</OptionalLabel>
                      <FormControl>
                        <Input
                          placeholder="e.g. a1b2c3d4..."
                          autoComplete="off"
                          className="font-mono"
                          disabled={!projectId}
                          {...field}
                        />
                      </FormControl>
                      <FormDescription>
                        The runner checks out this exact commit instead of the
                        branch head.
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ) : null}

              <FormField
                control={form.control}
                name="changelog"
                render={({ field }) => (
                  <FormItem>
                    <OptionalLabel>What changed?</OptionalLabel>
                    {field.value === undefined &&
                    changelogPreviewQuery.data?.markdown &&
                    changelogPreview(changelogPreviewQuery.data.markdown).more >
                      0 ? (
                      <DraftedChangelog
                        markdown={changelogPreviewQuery.data.markdown}
                        onEdit={() =>
                          field.onChange(changelogPreviewQuery.data?.markdown)
                        }
                        onClear={() => field.onChange('')}
                      />
                    ) : (
                      <FormControl>
                        <Textarea
                          {...field}
                          value={
                            field.value ??
                            changelogPreviewQuery.data?.markdown ??
                            ''
                          }
                          disabled={!selectedPipelineId}
                          placeholder={
                            selectedPipelineId
                              ? 'No changes found since the previous build.'
                              : 'Choose a pipeline to draft release notes from its commits.'
                          }
                          rows={5}
                        />
                      </FormControl>
                    )}
                    <FormDescription>
                      {!selectedPipelineId
                        ? 'Drafted from commit titles and authors once a pipeline is chosen. Markdown supported.'
                        : changelogPreviewQuery.isFetching
                          ? 'Drafting from commits since the previous successful build…'
                          : changelogPreviewQuery.error
                            ? 'Could not generate a draft. You can still write one.'
                            : changelogPreviewQuery.data?.target_commit
                              ? `Drafted from commits up to ${changelogPreviewQuery.data.target_commit.slice(0, 7)}. The build uses the latest commit on the branch when it starts. Markdown supported.`
                              : 'Drafted from commit titles and authors. Edit or clear it before running. Markdown supported.'}
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
              {selectedPipelineId ? (
                <TriggerBuildStoreSection
                  form={form}
                  pipelineId={selectedPipelineId}
                  changelogMarkdown={changelogPreviewQuery.data?.markdown}
                  enabled={open}
                  filePublishes={chosenWorkflow?.publishes ?? []}
                />
              ) : null}
            </div>
            {/* Outside the scroll area so the footer never covers them. */}
            <Stack gap={2} className="shrink-0 px-4 pb-4">
              {signingError ? (
                <Alert variant="destructive" role="alert">
                  <AlertDescription>{signingError}</AlertDescription>
                </Alert>
              ) : null}
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
                        ? 'Runners are paused. This build will wait until an owner or admin resumes them.'
                        : runnersQuery.data?.online_total === 0
                          ? 'No runner is online. This build will wait for one to connect.'
                          : 'A runner is online. Toolchain, repository access and signing are checked when the build runs.'}{' '}
                  <Link
                    to="/settings/runners"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="underline underline-offset-4"
                  >
                    View runners
                    <span className="sr-only"> (opens in a new tab)</span>
                  </Link>
                  .
                </AlertDescription>
              </Alert>
            </Stack>

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
              pending={resolvingHead || createBuildMutation.isPending}
            />
          </form>
        </Form>
      </DrawerContent>
    </Drawer>
  )
}

function BranchHeadDescription({
  title,
  commit,
}: {
  title: string | undefined
  commit: string | undefined
}) {
  if (!title || !commit) return null
  return (
    <FormDescription className="truncate">
      Latest: <span className="font-mono">{commit.slice(0, 7)}</span> {title}
    </FormDescription>
  )
}

function RecentRunChips({
  runs,
  showProject,
  showPipeline,
  onPick,
}: {
  runs: Array<RecentRun>
  showProject: boolean
  showPipeline: boolean
  onPick: (run: RecentRun) => void
}) {
  if (runs.length === 0 || (!showProject && !showPipeline && runs.length < 2))
    return null
  return (
    <Stack gap={2}>
      <Text variant="caption">Recent</Text>
      <Row gap={2} className="flex-wrap">
        {runs.slice(0, 3).map((run) => (
          <Badge
            key={`${run.pipelineId}:${run.branch}`}
            variant="outline"
            render={<button type="button" onClick={() => onPick(run)} />}
            className="max-w-full cursor-pointer truncate"
          >
            {[
              showProject ? run.projectName : undefined,
              showPipeline ? run.pipelineName : undefined,
              run.branch,
            ]
              .filter(Boolean)
              .join(' · ')}
          </Badge>
        ))}
      </Row>
    </Stack>
  )
}
