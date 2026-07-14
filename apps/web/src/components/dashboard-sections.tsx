import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, ArrowRight01Icon, Link04Icon } from '@hugeicons/core-free-icons'

import type { Build, Project, RuntimeMode } from '@/lib/types'
import { getStatusVariant } from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
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
        <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          Getting Started
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
                    <HugeiconsIcon icon={Link04Icon} />
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
                  : 'Pick a repository from a connected source or use a local path.'}
              </p>
              {canWriteProjects && !noConnectedSources ? (
                <Button
                  size="sm"
                  render={<Link to="/projects" search={{ openCreate: '1' }} />}
                  nativeButton={false}
                >
                  <HugeiconsIcon icon={Add01Icon} />
                  Create project
                </Button>
              ) : (
                <p className="text-xs text-muted-foreground">
                  Ask a developer or admin to create a project.
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
  isLoading,
  onOpenBuild,
  projects,
}: {
  builds: Array<Build>
  isLoading: boolean
  onOpenBuild: (buildId: string) => void
  projects: Array<Project>
}) {
  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
          Recent Builds
        </h2>
        <Button
          variant="ghost"
          size="sm"
          render={<Link to="/builds" />}
          nativeButton={false}
        >
          View all
          <HugeiconsIcon icon={ArrowRight01Icon} />
        </Button>
      </div>

      {isLoading ? (
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
        <Card>
          <CardContent>
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
                {builds.map((build) => {
                  const projectName =
                    projects.find((project) => project.id === build.project_id)
                      ?.name ?? build.project_id.slice(0, 8)
                  return (
                    <TableRow
                      key={build.id}
                      className="group cursor-pointer"
                      onClick={() => onOpenBuild(build.id)}
                    >
                      <TableCell className="font-mono text-sm group-hover:underline">
                        #{build.build_number}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {projectName}
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
                        {build.commit_sha ? build.commit_sha.slice(0, 8) : 'n/a'}
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {relativeTime(build.created_at)}
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </section>
  )
}
