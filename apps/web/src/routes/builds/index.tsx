import { For, Match, Show, Switch } from 'solid-js'
import { Link, createFileRoute } from '@tanstack/solid-router'
import { Add01Icon, PlayIcon } from '@hugeicons/core-free-icons'

import { HugeIcon } from '@/components/huge-icon'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
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
import { useProjects } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { getStatusVariant } from '@/lib/status-variants'

export const Route = createFileRoute('/builds/')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  staticData: { breadcrumbLabel: 'Builds' },
  component: BuildsListPage,
})

function BuildsListPage() {
  const buildsQuery = useBuilds({ limit: 100 })
  const projectsQuery = useProjects({ limit: 200 })
  const setupStatusQuery = useSetupStatus()

  const canTriggerBuild = useHasPermission('builds', 'write')
  const canWriteProjects = useHasPermission('projects', 'write')
  const canWriteIntegrations = useHasPermission('integrations', 'write')

  const builds = () => buildsQuery.data?.builds ?? []
  const projects = () => projectsQuery.data?.projects ?? []

  const runtimeMode = () => setupStatusQuery.data?.runtime_mode ?? 'local'
  const missingProjects = () =>
    !projectsQuery.isLoading && !projectsQuery.error && projects().length === 0
  const isLoading = () => buildsQuery.isLoading || projectsQuery.isLoading
  const error = () => buildsQuery.error ?? projectsQuery.error

  return (
    <PageLayout width="wide">
      <PageMeta title="Builds" noindex />
      <PageHeader
        title="Builds"
        description="Queue, execution, and historical run inventory across projects."
        actions={
          !missingProjects() && canTriggerBuild ? (
            <Link to="/projects">
              <Button>
                <HugeIcon icon={PlayIcon} size={16} />
                Run Build
              </Button>
            </Link>
          ) : undefined
        }
      />

      <Show when={isLoading()}>
        <Card>
          <CardContent class="space-y-3">
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
            <Skeleton class="h-10 w-full" />
          </CardContent>
        </Card>
      </Show>

      <Show when={!!error()}>
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load builds: {error()?.message}
          </AlertDescription>
        </Alert>
      </Show>

      <Show when={!isLoading() && !error()}>
        <Switch>
          <Match when={missingProjects()}>
            <Card>
              <CardHeader>
                <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                  Create Project First
                </CardTitle>
              </CardHeader>
              <CardContent class="space-y-4">
                <p class="text-sm text-muted-foreground">
                  {runtimeMode() === 'local'
                    ? 'Builds run through pipelines under projects. Create your first project from a local Git repository.'
                    : 'Builds run through pipelines under projects. Create your first project before triggering builds.'}
                </p>

                <Show
                  when={canWriteProjects}
                  fallback={
                    <p class="text-xs text-muted-foreground">
                      Ask an owner/admin/developer to create the first project.
                    </p>
                  }
                >
                  <Link to="/projects">
                    <Button>
                      <HugeIcon icon={Add01Icon} size={16} />
                      Go To Projects
                    </Button>
                  </Link>
                </Show>

                <Show when={runtimeMode() === 'remote'}>
                  <Show
                    when={canWriteIntegrations}
                    fallback={
                      <p class="text-xs text-muted-foreground">
                        Ask an owner/admin to connect a source.
                      </p>
                    }
                  >
                    <Link to="/settings/integrations">
                      <Button variant="outline">Connect Source</Button>
                    </Link>
                  </Show>
                </Show>
              </CardContent>
            </Card>
          </Match>

          <Match when>
            <Card>
              <CardHeader>
                <div class="flex items-center justify-between">
                  <CardTitle class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
                    Build queue and history
                  </CardTitle>
                  <span class="text-xs text-muted-foreground">
                    {builds().length} total
                  </span>
                </div>
              </CardHeader>
              <CardContent>
                <Show
                  when={builds().length > 0}
                  fallback={
                    <div class="space-y-2 py-6">
                      <p class="text-sm text-muted-foreground">No builds yet.</p>
                      <Show when={canTriggerBuild}>
                        <Link to="/projects">
                          <Button size="sm">
                            <HugeIcon icon={PlayIcon} size={14} />
                            Trigger first build
                          </Button>
                        </Link>
                      </Show>
                    </div>
                  }
                >
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Build</TableHead>
                        <TableHead>Status</TableHead>
                        <TableHead>Trigger</TableHead>
                        <TableHead>Branch</TableHead>
                        <TableHead>Commit</TableHead>
                        <TableHead>Created</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      <For each={builds()}>
                        {(build) => (
                          <TableRow>
                            <TableCell>
                              <div>
                                <Link
                                  to="/builds/$buildId"
                                  params={{ buildId: build.id }}
                                  class="font-mono text-sm hover:underline"
                                >
                                  #{build.build_number}
                                </Link>
                                <p class="font-mono text-[11px] text-muted-foreground">
                                  {build.id.slice(0, 8)}
                                </p>
                              </div>
                            </TableCell>
                            <TableCell>
                              <Badge variant={getStatusVariant(build.status)}>
                                {build.status}
                              </Badge>
                            </TableCell>
                            <TableCell>
                              <div class="flex items-center gap-2">
                                <Badge variant="outline">{build.trigger_type}</Badge>
                                {build.trigger_actor ? (
                                  <span class="text-xs text-muted-foreground">
                                    by {build.trigger_actor}
                                  </span>
                                ) : null}
                              </div>
                            </TableCell>
                            <TableCell class="font-mono text-xs text-muted-foreground">
                              {build.branch ?? 'n/a'}
                            </TableCell>
                            <TableCell class="font-mono text-xs text-muted-foreground">
                              {build.commit_sha
                                ? build.commit_sha.slice(0, 10)
                                : 'n/a'}
                            </TableCell>
                            <TableCell class="text-sm text-muted-foreground">
                              {new Date(build.created_at * 1000).toLocaleString()}
                            </TableCell>
                          </TableRow>
                        )}
                      </For>
                    </TableBody>
                  </Table>
                </Show>
              </CardContent>
            </Card>
          </Match>
        </Switch>
      </Show>
    </PageLayout>
  )
}
