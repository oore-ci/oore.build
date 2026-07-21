import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon, ArrowRight01Icon, Link04Icon } from '@hugeicons/core-free-icons'

import type { Build, Project, RuntimeMode } from '@/lib/types'
import { getStatusVariant } from '@/lib/status-variants'
import { relativeTime } from '@/lib/format-utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
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
    <Card className="overflow-hidden">
      <div className="border-b px-4 py-3">
        <p className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/60">
          Getting Started
        </p>
      </div>
      <CardContent className="p-4">
        <ol className="space-y-4 text-sm">
          {hasSourceStep ? (
            <li className="flex items-start gap-3">
              <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-[11px] font-semibold text-primary">
                1
              </span>
              <div className="space-y-1.5">
                <p className="font-medium">Connect a source</p>
                <p className="text-xs text-muted-foreground leading-relaxed">
                  Link GitHub or GitLab to import repositories and enable
                  webhook-triggered builds.
                </p>
                {canWriteIntegrations ? (
                  <Button
                    size="sm"
                    className="h-7 gap-1.5 text-xs"
                    render={<Link to={integrationConnectTo} />}
                    nativeButton={false}
                  >
                    <HugeiconsIcon icon={Link04Icon} size={13} />
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
            <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-[11px] font-semibold text-primary">
              {hasSourceStep ? '2' : '1'}
            </span>
            <div className="space-y-1.5">
              <p className="font-medium">Create a project</p>
              <p className="text-xs text-muted-foreground leading-relaxed">
                {runtimeMode === 'local'
                  ? 'Point to a local Flutter repository to get started.'
                  : 'Pick a repository from a connected source or use a local path.'}
              </p>
              {canWriteProjects && !noConnectedSources ? (
                <Button
                  size="sm"
                  className="h-7 gap-1.5 text-xs"
                  render={<Link to="/projects" search={{ openCreate: '1' }} />}
                  nativeButton={false}
                >
                  <HugeiconsIcon icon={Add01Icon} size={13} />
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
            <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-[11px] font-semibold text-primary">
              {hasSourceStep ? '3' : '2'}
            </span>
            <div className="space-y-1.5">
              <p className="font-medium">Add a pipeline</p>
              <p className="text-xs text-muted-foreground leading-relaxed">
                Configure which platforms to build (Android, iOS, macOS) and
                signing settings.
              </p>
            </div>
          </li>
          <li className="flex items-start gap-3">
            <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-[11px] font-semibold text-primary">
              {hasSourceStep ? '4' : '3'}
            </span>
            <div className="space-y-1.5">
              <p className="font-medium">Run your first build</p>
              <p className="text-xs text-muted-foreground leading-relaxed">
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
        <h2 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/60">
          Recent Builds
        </h2>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1 text-xs text-muted-foreground hover:text-foreground"
          render={<Link to="/builds" />}
          nativeButton={false}
        >
          View all
          <HugeiconsIcon icon={ArrowRight01Icon} size={13} />
        </Button>
      </div>

      {isLoading ? (
        <Card>
          <CardContent className="space-y-3 p-4">
            <Skeleton className="h-7 w-full" />
            <Skeleton className="h-7 w-full" />
            <Skeleton className="h-7 w-full" />
          </CardContent>
        </Card>
      ) : builds.length === 0 ? (
        <Card>
          <CardContent className="p-4">
            <p className="py-2 text-center text-sm text-muted-foreground">
              No builds yet.
            </p>
          </CardContent>
        </Card>
      ) : (
        <Card className="overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow className="border-b">
                <TableHead className="h-9 pl-4 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                  Build
                </TableHead>
                <TableHead className="h-9 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                  Project
                </TableHead>
                <TableHead className="h-9 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                  Status
                </TableHead>
                <TableHead className="h-9 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                  Branch
                </TableHead>
                <TableHead className="h-9 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                  Commit
                </TableHead>
                <TableHead className="h-9 pr-4 text-right text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                  When
                </TableHead>
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
                    <TableCell className="py-2.5 pl-4">
                      <span className="font-mono text-sm font-semibold text-primary group-hover:underline">
                        #{build.build_number}
                      </span>
                    </TableCell>
                    <TableCell className="py-2.5 text-sm text-muted-foreground">
                      {projectName}
                    </TableCell>
                    <TableCell className="py-2.5">
                      <Badge
                        variant={getStatusVariant(build.status)}
                        className="text-[10px]"
                      >
                        {build.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="py-2.5 font-mono text-xs text-muted-foreground">
                      {build.branch ?? 'n/a'}
                    </TableCell>
                    <TableCell className="py-2.5 font-mono text-xs text-muted-foreground">
                      {build.commit_sha ? build.commit_sha.slice(0, 8) : 'n/a'}
                    </TableCell>
                    <TableCell className="py-2.5 pr-4 text-right text-xs text-muted-foreground">
                      {relativeTime(build.created_at)}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </Card>
      )}
    </section>
  )
}
