import { For, Match, Show, Switch, createEffect, createSignal } from 'solid-js'
import { Link, createFileRoute, useNavigate } from '@tanstack/solid-router'
import { Add01Icon } from '@hugeicons/core-free-icons'

import { HugeIcon } from '@/components/huge-icon'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { FormError, FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { useIntegrations } from '@/hooks/use-integrations'
import { useHasPermission } from '@/hooks/use-permissions'
import { useCreateProject, useProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { getActiveInstanceOrRedirect, requireAuthOrRedirect } from '@/lib/instance-context'
import { relativeTime } from '@/lib/format-utils'
import { PageMeta } from '@/lib/seo'

export const Route = createFileRoute('/projects/')({
  staticData: { breadcrumbLabel: 'Projects' },
  validateSearch: (search: Record<string, unknown>): { openCreate?: string } => ({
    openCreate: (search.openCreate as string) || undefined,
  }),
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: ProjectsListPage,
})

function ProjectsListPage() {
  const search = Route.useSearch()
  const navigate = useNavigate()

  const projectsQuery = useProjects({ limit: 100 })
  const integrationsQuery = useIntegrations()
  const setupStatusQuery = useSetupStatus()
  const createProjectMutation = useCreateProject()

  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteIntegrations = useHasPermission('integrations', 'write')

  const [showCreate, setShowCreate] = createSignal(false)
  const [createName, setCreateName] = createSignal('')
  const [createRepoPath, setCreateRepoPath] = createSignal('')
  const [createError, setCreateError] = createSignal<string | null>(null)

  createEffect(() => {
    if (search().openCreate !== '1') return
    setShowCreate(true)
    void navigate({ to: '/projects', search: {}, replace: true })
  })

  const projects = () => projectsQuery.data?.projects ?? []
  const integrations = () => integrationsQuery.data?.integrations ?? []
  const runtimeMode = () => setupStatusQuery.data?.runtime_mode ?? 'local'

  const activeIntegrationsCount = () =>
    integrations().filter((integration) => integration.status === 'active').length

  const integrationsResolved = () =>
    !integrationsQuery.isLoading && !integrationsQuery.error

  const noConnectedSources = () =>
    runtimeMode() === 'remote' &&
    integrationsResolved() &&
    activeIntegrationsCount() === 0

  const projectsLoading = () => projectsQuery.isLoading || integrationsQuery.isLoading
  const projectsError = () => projectsQuery.error ?? integrationsQuery.error

  const handleCreateProject = () => {
    const name = createName().trim()
    const localRepositoryPath = createRepoPath().trim()

    if (!name) {
      setCreateError('Project name is required.')
      return
    }

    if (!localRepositoryPath) {
      setCreateError('Repository URL/path is required.')
      return
    }

    setCreateError(null)
    createProjectMutation.mutate(
      {
        name,
        local_repository_path: localRepositoryPath,
      },
      {
        onSuccess: () => {
          setCreateName('')
          setCreateRepoPath('')
          setShowCreate(false)
        },
        onError: (error) => {
          setCreateError(error instanceof Error ? error.message : 'Failed to create project')
        },
      },
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="Projects" noindex />
      <PageHeader
        title="Projects"
        description="Repository and pipeline entry points for your build system."
        actions={
          canWriteProjects ? (
            <Button onClick={() => setShowCreate((current) => !current)}>
              <HugeIcon icon={Add01Icon} size={16} />
              New Project
            </Button>
          ) : undefined
        }
      />

      <Show when={showCreate() && canWriteProjects}>
        <Card>
          <CardHeader>
            <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Create Project
            </CardTitle>
          </CardHeader>
          <CardContent class="grid gap-3 md:grid-cols-[1fr_1fr_auto_auto]">
            <FormField>
              <Input
                value={createName()}
                onInput={(event) => setCreateName(event.currentTarget.value)}
                placeholder="Project name"
              />
            </FormField>
            <FormField>
              <Input
                value={createRepoPath()}
                onInput={(event) => setCreateRepoPath(event.currentTarget.value)}
                placeholder="Repository URL or local path"
              />
            </FormField>
            <Button
              onClick={handleCreateProject}
              disabled={createProjectMutation.isPending}
            >
              {createProjectMutation.isPending ? 'Creating...' : 'Create'}
            </Button>
            <Button variant="outline" onClick={() => setShowCreate(false)}>
              Cancel
            </Button>

            <Show when={createError()}>
              <div class="md:col-span-4">
                <FormError>{createError() ?? ''}</FormError>
              </div>
            </Show>
          </CardContent>
        </Card>
      </Show>

      <Show when={projectsLoading()}>
        <Card>
          <CardContent class="space-y-3">
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
          </CardContent>
        </Card>
      </Show>

      <Show when={!!projectsError()}>
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load projects: {projectsError()?.message}
          </AlertDescription>
        </Alert>
      </Show>

      <Show when={!projectsLoading() && !projectsError()}>
        <Switch>
          <Match when={projects().length === 0}>
            <Card>
              <CardHeader>
                <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                  Create Your First Project
                </CardTitle>
              </CardHeader>
              <CardContent class="space-y-4">
                <p class="text-sm text-muted-foreground">
                  {runtimeMode() === 'local'
                    ? 'Choose a local Git repository to create your first project.'
                    : noConnectedSources()
                      ? 'Create a project from a local repository path, or connect a source to pick from synced repositories.'
                      : 'Create a project from a connected source repository to define pipelines and start builds.'}
                </p>

                <div class="flex flex-wrap items-center gap-2">
                  <Show
                    when={canWriteProjects}
                    fallback={
                      <p class="text-xs text-muted-foreground">
                        Owner/Admin/Developer required to create projects.
                      </p>
                    }
                  >
                    <Button onClick={() => setShowCreate(true)}>
                      <HugeIcon icon={Add01Icon} size={14} />
                      Create Project
                    </Button>
                  </Show>

                  <Show when={noConnectedSources()}>
                    <Show
                      when={canWriteIntegrations}
                      fallback={
                        <p class="text-xs text-muted-foreground">
                          Owner/Admin required to connect a source.
                        </p>
                      }
                    >
                      <Link to="/settings/integrations">
                        <Button variant="outline">Connect Source</Button>
                      </Link>
                    </Show>
                  </Show>
                </div>
              </CardContent>
            </Card>
          </Match>

          <Match when>
            <Card>
              <CardHeader>
                <div class="flex items-center justify-between">
                  <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                    Project inventory
                  </CardTitle>
                  <span class="text-xs text-muted-foreground">
                    {projects().length} total
                  </span>
                </div>
              </CardHeader>
              <CardContent>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Project</TableHead>
                      <TableHead>Default branch</TableHead>
                      <TableHead>Description</TableHead>
                      <TableHead>Updated</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    <For each={projects()}>
                      {(project) => (
                        <TableRow>
                          <TableCell>
                            <div>
                              <Link
                                to="/projects/$projectId"
                                params={{ projectId: project.id }}
                                class="font-medium hover:underline"
                              >
                                {project.name}
                              </Link>
                              <p class="font-mono text-[11px] text-muted-foreground">
                                {project.id.slice(0, 8)}
                              </p>
                            </div>
                          </TableCell>
                          <TableCell class="font-mono text-xs text-muted-foreground">
                            {project.default_branch ?? 'main'}
                          </TableCell>
                          <TableCell class="text-sm text-muted-foreground">
                            {project.description ?? 'No description'}
                          </TableCell>
                          <TableCell class="text-sm text-muted-foreground">
                            {relativeTime(project.updated_at)}
                          </TableCell>
                        </TableRow>
                      )}
                    </For>
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </Match>
        </Switch>
      </Show>
    </PageLayout>
  )
}
