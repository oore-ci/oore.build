import { Link } from '@tanstack/react-router'
import {
  Plus as Add01Icon,
  ArrowRight as ArrowRight01Icon,
  Link2 as Link04Icon,
} from 'lucide-react'

import type { Build, Project, RuntimeMode } from '@/lib/types'
import { getStatusVariant } from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
import { Badge } from '@/components/ui/badge'
import { Alert, AlertDescription } from '@/components/ui/alert'
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

export function DashboardGettingStarted({
  canWriteIntegrations,
  canWriteProjects,
  integrationConnectTo,
  noConnectedSources,
  runtimeMode,
}: {
  canWriteIntegrations: boolean
  canWriteProjects: boolean
  integrationConnectTo: '/settings/integrations'
  noConnectedSources: boolean
  runtimeMode: RuntimeMode
}) {
  const hasSourceStep = runtimeMode === 'remote' && noConnectedSources
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-medium text-muted-foreground">
          Getting started
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ol className="space-y-3 text-sm">
          {hasSourceStep ? (
            <li className="flex items-start gap-3">
              <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
                1
              </span>
              <div className="space-y-1.5">
                <p className="font-medium">Connect a source</p>
                <p className="text-xs text-muted-foreground">
                  Link GitHub or GitLab to import repositories and enable
                  webhook-triggered builds.
                </p>
                {canWriteIntegrations ? (
                  <Button
                    size="sm"
                    render={<Link to={integrationConnectTo} />}
                    nativeButton={false}
                  >
                    <Link04Icon />
                    Connect source
                  </Button>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    Ask an admin to connect a source.
                  </p>
                )}
              </div>
            </li>
          ) : null}
          <li className="flex items-start gap-3">
            <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
              {hasSourceStep ? '2' : '1'}
            </span>
            <div className="space-y-1.5">
              <p className="font-medium">Create a project</p>
              <p className="text-xs text-muted-foreground">
                {runtimeMode === 'local'
                  ? 'Point to a local Flutter repository to get started.'
                  : 'Pick a repository from a connected source.'}
              </p>
              {canWriteProjects && !noConnectedSources ? (
                <Button
                  size="sm"
                  render={<Link to="/projects" search={{ openCreate: '1' }} />}
                  nativeButton={false}
                >
                  <Add01Icon />
                  Create project
                </Button>
              ) : (
                <p className="text-xs text-muted-foreground">
                  Ask an owner or admin to create a project.
                </p>
              )}
            </div>
          </li>
          <li className="flex items-start gap-3">
            <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
              {hasSourceStep ? '3' : '2'}
            </span>
            <div className="space-y-1.5">
              <p className="font-medium">Add a pipeline</p>
              <p className="text-xs text-muted-foreground">
                Configure which platforms to build (Android, iOS, macOS) and
                signing settings.
              </p>
            </div>
          </li>
          <li className="flex items-start gap-3">
            <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center border text-[11px] font-medium text-muted-foreground">
              {hasSourceStep ? '4' : '3'}
            </span>
            <div className="space-y-1.5">
              <p className="font-medium">Run your first build</p>
              <p className="text-xs text-muted-foreground">
                Trigger a build manually or push to your repository to start
                automatically.
              </p>
            </div>
          </li>
        </ol>
      </CardContent>
    </Card>
  )
}

export function DashboardRecentBuilds({
  builds,
  error,
  isLoading,
  projects,
  onRetry,
}: {
  builds: Array<Build>
  error?: Error | null
  isLoading: boolean
  projects: Array<Project>
  onRetry: () => void
}) {
  const projectNames = new Map(
    projects.map((project) => [project.id, project.name]),
  )

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-muted-foreground">
          Recent builds
        </h2>
        <Button
          variant="ghost"
          size="sm"
          render={<Link to="/builds" />}
          nativeButton={false}
        >
          View all
          <ArrowRight01Icon />
        </Button>
      </div>

      {error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex items-center justify-between gap-3">
            <span>Build activity could not be loaded.</span>
            <Button variant="outline" size="sm" onClick={onRetry}>
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </CardContent>
        </Card>
      ) : builds.length === 0 ? (
        <Card>
          <CardContent>
            <div className="py-4 text-center">
              <p className="text-sm text-muted-foreground">No builds yet.</p>
            </div>
          </CardContent>
        </Card>
      ) : (
        <Card size="sm">
          <CardContent>
            <div className="divide-y sm:hidden">
              {builds.map((build) => (
                <Link
                  key={build.id}
                  to="/builds/$buildId"
                  params={{ buildId: build.id }}
                  className="flex min-h-16 items-center justify-between gap-3 py-3 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">
                      {projectNames.get(build.project_id) ??
                        build.context?.project_name ??
                        build.project_id.slice(0, 8)}{' '}
                      <span className="font-mono">#{build.build_number}</span>
                    </p>
                    <p className="mt-1 truncate text-xs text-muted-foreground">
                      {build.branch ?? 'No branch'} ·{' '}
                      {relativeTime(build.created_at)}
                    </p>
                  </div>
                  <Badge variant={getStatusVariant(build.status)}>
                    {build.status}
                  </Badge>
                </Link>
              ))}
            </div>
            <div className="hidden sm:block">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Build</TableHead>
                    <TableHead>Project</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Branch</TableHead>
                    <TableHead>Commit</TableHead>
                    <TableHead>When</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {builds.map((build) => (
                    <TableRow key={build.id}>
                      <TableCell className="font-mono text-sm">
                        <Link
                          to="/builds/$buildId"
                          params={{ buildId: build.id }}
                          className="hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                        >
                          #{build.build_number}
                        </Link>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {projectNames.get(build.project_id) ??
                          build.context?.project_name ??
                          build.project_id.slice(0, 8)}
                      </TableCell>
                      <TableCell>
                        <Badge variant={getStatusVariant(build.status)}>
                          {build.status}
                        </Badge>
                      </TableCell>
                      <TableCell className="font-mono text-xs text-muted-foreground">
                        {build.branch ?? 'n/a'}
                      </TableCell>
                      <TableCell className="font-mono text-xs text-muted-foreground">
                        {build.commit_sha
                          ? build.commit_sha.slice(0, 8)
                          : 'n/a'}
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {relativeTime(build.created_at)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      )}
    </section>
  )
}
