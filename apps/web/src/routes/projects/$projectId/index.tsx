import { For, Match, Show, Switch, createEffect, createMemo, createSignal } from 'solid-js'
import { Link, createFileRoute, useNavigate } from '@tanstack/solid-router'
import {
  Add01Icon,
  ArrowRight01Icon,
  Delete02Icon,
  Link01Icon,
  PlayIcon,
} from '@hugeicons/core-free-icons'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { FormError, FormField } from '@/components/ui/form'
import { HugeIcon } from '@/components/huge-icon'
import { Input } from '@/components/ui/input'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
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
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { usePipelines, useUpdatePipeline } from '@/hooks/use-pipelines'
import {
  useDeleteProject,
  useProject,
  useUpdateProject,
} from '@/hooks/use-projects'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import type { Pipeline } from '@/lib/types'
import { PageMeta } from '@/lib/seo'
import { getPipelineStatusVariant, getStatusVariant } from '@/lib/status-variants'
import { toast } from '@/components/ui/sonner'

const TAB_VALUES = ['pipelines', 'builds', 'settings'] as const
type TabValue = (typeof TAB_VALUES)[number]

export const Route = createFileRoute('/projects/$projectId/')({
  staticData: { breadcrumbLabel: 'Details' },
  validateSearch: (
    search: Record<string, unknown>,
  ): { tab?: TabValue } => ({
    tab: TAB_VALUES.includes(search.tab as TabValue)
      ? (search.tab as TabValue)
      : undefined,
  }),
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ProjectDetailPage,
})

function relativeTime(epochSeconds: number): string {
  const diffSecs = Math.floor(Date.now() / 1000) - epochSeconds
  if (diffSecs < 5) return 'just now'
  if (diffSecs < 60) return `${diffSecs}s ago`
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  const days = Math.floor(hrs / 24)
  return `${days}d ago`
}

function ProjectDetailPage() {
  const params = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()

  const projectQuery = useProject(params().projectId)
  const pipelinesQuery = usePipelines(params().projectId)
  const buildsQuery = useBuilds({ project_id: params().projectId, limit: 20 })
  const deleteMutation = useDeleteProject()
  const updateMutation = useUpdateProject()
  const updatePipeline = useUpdatePipeline()

  const canDeleteProjects = useHasPermission('projects', 'delete')
  const canWritePipelines = useHasPermission('pipelines', 'write')
  const canTriggerBuild = useHasPermission('builds', 'write')

  const project = () => projectQuery.data?.project
  const pipelines = () => pipelinesQuery.data?.pipelines ?? []
  const builds = () => buildsQuery.data?.builds ?? []
  const projectHasSource = () => !!project()?.repository_id

  const activeTab = () => search().tab ?? 'pipelines'
  const label = () => project()?.name ?? 'Project Details'

  const [deleteOpen, setDeleteOpen] = createSignal(false)
  const [name, setName] = createSignal('')
  const [description, setDescription] = createSignal('')
  const [defaultBranch, setDefaultBranch] = createSignal('')
  const [formError, setFormError] = createSignal<string | null>(null)

  const lastBuildByPipeline = createMemo(() => {
    const map = new Map<string, { status: string; time: number }>()
    for (const build of builds()) {
      if (build.pipeline_id && !map.has(build.pipeline_id)) {
        map.set(build.pipeline_id, {
          status: build.status,
          time: build.queued_at,
        })
      }
    }
    return map
  })

  createEffect(() => {
    if (!project()) return
    setName(project()!.name)
    setDescription(project()!.description ?? '')
    setDefaultBranch(project()!.default_branch ?? '')
  })

  function setTab(value: TabValue) {
    void navigate({
      to: '/projects/$projectId',
      params: { projectId: params().projectId },
      search: value === 'pipelines' ? {} : { tab: value },
      replace: true,
    })
  }

  function handleDelete() {
    deleteMutation.mutate(params().projectId, {
      onSuccess: () => {
        toast.success('Project deleted')
        void navigate({ to: '/projects' })
      },
      onError: (error) => {
        toast.error(
          error instanceof Error ? error.message : 'Failed to delete project',
        )
      },
    })
  }

  function handleSaveProject() {
    setFormError(null)
    const nextName = name().trim()

    if (!nextName) {
      setFormError('Name is required')
      return
    }

    updateMutation.mutate(
      {
        projectId: params().projectId,
        data: {
          name: nextName,
          description: description().trim() || undefined,
          default_branch: defaultBranch().trim() || undefined,
        },
      },
      {
        onSuccess: () => {
          toast.success('Project updated')
        },
        onError: (error) => {
          setFormError(
            error instanceof Error ? error.message : 'Failed to update project',
          )
        },
      },
    )
  }

  function handleTogglePipeline(pipeline: Pipeline) {
    updatePipeline.mutate(
      {
        pipelineId: pipeline.id,
        data: {
          enabled: !pipeline.enabled,
        },
      },
      {
        onSuccess: () => {
          toast.success(pipeline.enabled ? 'Pipeline disabled' : 'Pipeline enabled')
        },
        onError: (error) => {
          toast.error(
            error instanceof Error ? error.message : 'Failed to update pipeline',
          )
        },
      },
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title={label()} noindex />

      <Switch>
        <Match when={projectQuery.isLoading}>
          <Skeleton class="h-8 w-56" />
          <Skeleton class="h-10 w-72" />
          <Skeleton class="h-56 w-full" />
        </Match>

        <Match when={!!projectQuery.error}>
          <Alert variant="destructive">
            <AlertDescription>
              Failed to load project: {projectQuery.error?.message}
            </AlertDescription>
          </Alert>
        </Match>

        <Match when={!!project()}>
          <PageHeader
            title={project()!.name}
            back={{ to: '/projects', label: 'Projects' }}
            description={project()!.description}
            meta={
              <>
                {project()!.default_branch ? (
                  <Badge variant="outline" class="font-mono text-[11px]">
                    {project()!.default_branch}
                  </Badge>
                ) : null}
                <span>Updated {relativeTime(project()!.updated_at)}</span>
              </>
            }
            actions={
              canTriggerBuild || canDeleteProjects ? (
                <>
                  {canTriggerBuild ? (
                    <Link to="/builds">
                      <Button
                        disabled={pipelines().length === 0 || !projectHasSource()}
                      >
                        <HugeIcon icon={PlayIcon} size={16} />
                        Run Build
                      </Button>
                    </Link>
                  ) : null}

                  {canDeleteProjects ? (
                    <Button
                      variant="destructive"
                      onClick={() => setDeleteOpen(true)}
                    >
                      <HugeIcon icon={Delete02Icon} size={16} />
                      Delete
                    </Button>
                  ) : null}
                </>
              ) : undefined
            }
          />

          <Show when={!projectHasSource()}>
            <Alert variant="destructive">
              <AlertDescription>
                This project has no linked source repository. Link a repository
                before triggering builds.
              </AlertDescription>
            </Alert>
          </Show>

          <div class="space-y-4">
            <div class="flex items-center gap-2 border-b">
              <TabButton
                active={activeTab() === 'pipelines'}
                onClick={() => setTab('pipelines')}
                label={`Pipelines${pipelines().length > 0 ? ` (${pipelines().length})` : ''}`}
              />
              <TabButton
                active={activeTab() === 'builds'}
                onClick={() => setTab('builds')}
                label={`Builds${builds().length > 0 ? ` (${builds().length})` : ''}`}
              />
              <TabButton
                active={activeTab() === 'settings'}
                onClick={() => setTab('settings')}
                label="Settings"
              />
            </div>

            <Show when={activeTab() === 'pipelines'}>
              <div class="space-y-4 pt-2">
                <Show when={canWritePipelines}>
                  <div class="flex justify-end">
                    <Link
                      to="/projects/$projectId/pipelines/new"
                      params={{ projectId: params().projectId }}
                    >
                      <Button size="sm">
                        <HugeIcon icon={Add01Icon} size={14} />
                        Add Pipeline
                      </Button>
                    </Link>
                  </div>
                </Show>

                <Show
                  when={pipelines().length > 0}
                  fallback={
                    <p class="py-6 text-center text-sm text-muted-foreground">
                      No pipelines yet. Add one to start building.
                    </p>
                  }
                >
                  <For each={pipelines()}>
                    {(pipeline) => {
                      const lastBuild = lastBuildByPipeline().get(pipeline.id)
                      return (
                        <Card>
                          <CardContent>
                            <div class="flex items-start justify-between gap-4">
                              <div class="min-w-0 space-y-2">
                                <div class="flex items-center gap-2.5">
                                  <Link
                                    to="/projects/$projectId/pipelines/$pipelineId"
                                    params={{
                                      projectId: params().projectId,
                                      pipelineId: pipeline.id,
                                    }}
                                    class="text-sm font-semibold hover:underline"
                                  >
                                    {pipeline.name}
                                  </Link>
                                  <Badge variant={getPipelineStatusVariant(pipeline.enabled)}>
                                    {pipeline.enabled ? 'enabled' : 'disabled'}
                                  </Badge>
                                </div>

                                <div class="flex flex-wrap items-center gap-1.5">
                                  <For each={pipeline.execution_config.platforms}>
                                    {(platform) => (
                                      <Badge variant="outline" class="text-[11px]">
                                        {platform}
                                      </Badge>
                                    )}
                                  </For>
                                  <For each={pipeline.trigger_config.events}>
                                    {(event) => (
                                      <Badge variant="secondary" class="text-[11px]">
                                        {event}
                                      </Badge>
                                    )}
                                  </For>
                                </div>
                              </div>

                              <span class="shrink-0 pt-0.5 text-xs text-muted-foreground">
                                {lastBuild ? (
                                  <>
                                    Last build: <span class="font-medium">{lastBuild.status}</span>{' '}
                                    {relativeTime(lastBuild.time)}
                                  </>
                                ) : (
                                  'No builds'
                                )}
                              </span>
                            </div>

                            <div class="mt-4 flex flex-wrap items-center gap-2 border-t pt-4">
                              <Show when={canTriggerBuild}>
                                <Link to="/builds">
                                  <Button size="sm">
                                    <HugeIcon icon={PlayIcon} size={14} />
                                    Run
                                  </Button>
                                </Link>
                              </Show>

                              <Show when={canWritePipelines}>
                                <Link
                                  to="/projects/$projectId/pipelines/$pipelineId/edit"
                                  params={{
                                    projectId: params().projectId,
                                    pipelineId: pipeline.id,
                                  }}
                                >
                                  <Button size="sm" variant="outline">
                                    Edit
                                  </Button>
                                </Link>
                                <Button
                                  size="sm"
                                  variant="outline"
                                  onClick={() => handleTogglePipeline(pipeline)}
                                  disabled={updatePipeline.isPending}
                                >
                                  {pipeline.enabled ? 'Disable' : 'Enable'}
                                </Button>
                              </Show>

                              <Link
                                to="/projects/$projectId/pipelines/$pipelineId"
                                params={{
                                  projectId: params().projectId,
                                  pipelineId: pipeline.id,
                                }}
                              >
                                <Button size="sm" variant="ghost">
                                  <HugeIcon icon={Link01Icon} size={14} />
                                  Permalink
                                </Button>
                              </Link>

                              <Link
                                to="/projects/$projectId/pipelines/$pipelineId"
                                params={{
                                  projectId: params().projectId,
                                  pipelineId: pipeline.id,
                                }}
                                class="ml-auto"
                              >
                                <Button size="sm" variant="ghost">
                                  <HugeIcon icon={ArrowRight01Icon} size={14} />
                                  Details
                                </Button>
                              </Link>
                            </div>
                          </CardContent>
                        </Card>
                      )
                    }}
                  </For>
                </Show>
              </div>
            </Show>

            <Show when={activeTab() === 'builds'}>
              <Card>
                <CardContent>
                  <Show
                    when={builds().length > 0}
                    fallback={
                      <p class="py-6 text-center text-sm text-muted-foreground">
                        No builds yet for this project.
                      </p>
                    }
                  >
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>Build</TableHead>
                          <TableHead>Status</TableHead>
                          <TableHead>Pipeline</TableHead>
                          <TableHead>Branch</TableHead>
                          <TableHead>Queued</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        <For each={builds()}>
                          {(build) => (
                            <TableRow>
                              <TableCell>
                                <Link
                                  to="/builds/$buildId"
                                  params={{ buildId: build.id }}
                                  class="font-mono text-sm hover:underline"
                                >
                                  #{build.build_number}
                                </Link>
                              </TableCell>
                              <TableCell>
                                <Badge variant={getStatusVariant(build.status)}>
                                  {build.status}
                                </Badge>
                              </TableCell>
                              <TableCell class="font-mono text-xs text-muted-foreground">
                                {build.pipeline_id.slice(0, 8)}
                              </TableCell>
                              <TableCell class="font-mono text-xs text-muted-foreground">
                                {build.branch ?? 'n/a'}
                              </TableCell>
                              <TableCell class="text-sm text-muted-foreground">
                                {relativeTime(build.queued_at)}
                              </TableCell>
                            </TableRow>
                          )}
                        </For>
                      </TableBody>
                    </Table>
                  </Show>
                </CardContent>
              </Card>
            </Show>

            <Show when={activeTab() === 'settings'}>
              <Card>
                <CardContent class="space-y-4">
                  <FormField label="Name">
                    <Input
                      value={name()}
                      onInput={(event) => setName(event.currentTarget.value)}
                    />
                  </FormField>

                  <FormField label="Description">
                    <Input
                      value={description()}
                      onInput={(event) => setDescription(event.currentTarget.value)}
                    />
                  </FormField>

                  <FormField label="Default Branch">
                    <Input
                      value={defaultBranch()}
                      onInput={(event) => setDefaultBranch(event.currentTarget.value)}
                    />
                  </FormField>

                  <Show when={formError()}>
                    <FormError>{formError() ?? ''}</FormError>
                  </Show>

                  <div class="flex justify-end">
                    <Button
                      onClick={handleSaveProject}
                      disabled={updateMutation.isPending}
                    >
                      {updateMutation.isPending ? (
                        <>
                          <Spinner class="size-4" />
                          Saving...
                        </>
                      ) : (
                        'Save'
                      )}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </Show>
          </div>

          <Show when={deleteOpen()}>
            <div class="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4">
              <Card class="w-full max-w-md">
                <CardContent class="space-y-4 pt-6">
                  <p class="text-base font-semibold">Delete project?</p>
                  <p class="text-sm text-muted-foreground">
                    This action cannot be undone. Pipelines and builds remain in
                    history but new runs will stop.
                  </p>
                  <div class="flex justify-end gap-2">
                    <Button variant="outline" onClick={() => setDeleteOpen(false)}>
                      Cancel
                    </Button>
                    <Button
                      variant="destructive"
                      onClick={handleDelete}
                      disabled={deleteMutation.isPending}
                    >
                      {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </div>
          </Show>
        </Match>
      </Switch>
    </PageLayout>
  )
}

function TabButton(props: {
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      class={`border-b-2 px-1.5 py-2 text-sm ${
        props.active
          ? 'border-foreground text-foreground'
          : 'border-transparent text-muted-foreground hover:text-foreground'
      }`}
      onClick={props.onClick}
    >
      {props.label}
    </button>
  )
}
