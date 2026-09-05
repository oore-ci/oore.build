import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ChevronRightIcon,
  CheckmarkCircle02Icon as CheckCircleIcon,
  Link04Icon,
  Add01Icon,
} from '@hugeicons/core-free-icons'

import DashboardBuildIncident from '@/components/dashboard-build-incident'
import { BuildItem } from '@/components/build-item'
import DashboardSystemStatus from '@/components/dashboard-system-status'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { ItemGroup } from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import type { RuntimeMode } from '@oore/client/models'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import type { Build } from '@oore/client/models'

export function DashboardGettingStarted({
  canWriteIntegrations,
  canWriteProjects,
  noConnectedSources,
  runtimeMode,
}: {
  canWriteIntegrations: boolean
  canWriteProjects: boolean
  noConnectedSources: boolean
  runtimeMode: RuntimeMode
}) {
  const scope = useFirstAppScope()
  const updateProgress = useFirstAppStore((state) => state.update)
  const hasSourceStep = runtimeMode === 'remote' && noConnectedSources

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Build and share your first app</CardTitle>
      </CardHeader>
      <CardContent>
        <ol className="flex flex-col gap-3 text-sm">
          {hasSourceStep ? (
            <li className="flex items-start gap-3">
              <Badge variant="outline" className="mt-0.5 size-5 px-0">
                1
              </Badge>
              <div className="flex flex-col gap-1.5">
                <p className="font-medium">Connect a source</p>
                <p className="text-xs text-muted-foreground">
                  Link GitHub or GitLab to import repositories and enable
                  webhook-triggered builds.
                </p>
                {canWriteIntegrations ? (
                  <Button
                    size="sm"
                    onClick={() =>
                      updateProgress(scope, { projectDraft: { name: '' } })
                    }
                    render={<Link to="/settings/integrations" />}
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
            <Badge variant="outline" className="mt-0.5 size-5 px-0">
              {hasSourceStep ? '2' : '1'}
            </Badge>
            <div className="flex flex-col gap-1.5">
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
                  <HugeiconsIcon icon={Add01Icon} />
                  Create project
                </Button>
              ) : (
                <p className="text-xs text-muted-foreground">
                  {canWriteProjects
                    ? 'Connect a source before you create a project.'
                    : 'Ask an owner or admin to create a project.'}
                </p>
              )}
            </div>
          </li>

          <li className="flex items-start gap-3">
            <Badge variant="outline" className="mt-0.5 size-5 px-0">
              {hasSourceStep ? '3' : '2'}
            </Badge>
            <div className="flex flex-col gap-1.5">
              <p className="font-medium">Add a pipeline</p>
              <p className="text-xs text-muted-foreground">
                Configure which platforms to build (Android, iOS, macOS) and
                signing settings.
              </p>
            </div>
          </li>

          <li className="flex items-start gap-3">
            <Badge variant="outline" className="mt-0.5 size-5 px-0">
              {hasSourceStep ? '4' : '3'}
            </Badge>
            <div className="flex flex-col gap-1.5">
              <p className="font-medium">Run and install your app</p>
              <p className="text-xs text-muted-foreground">
                Review and run the build, then open the app output to install it
                or share it with a tester.
              </p>
            </div>
          </li>
        </ol>
      </CardContent>
    </Card>
  )
}

const ACTIVITY_STATUS_PRIORITY = {
  running: 0,
  assigned: 1,
  scheduled: 2,
  queued: 3,
  failed: 4,
  timed_out: 5,
  canceled: 6,
  expired: 7,
  succeeded: 8,
} satisfies Record<Build['status'], number>

export function DashboardBuildOverview({
  activeBuilds,
  blockedBuilds,
  completedBuilds,
  error,
  isLoading,
  onlineRunners,
  noOnlineRunners,
  onRetry,
  recentBuilds,
  runnersError,
  runnersLoading,
  runningBuilds,
  statusCountsError,
  statusCountsLoading,
  successfulBuilds,
  totalRunners,
  waitingBuilds,
}: {
  activeBuilds: Array<Build>
  blockedBuilds: Array<Build>
  completedBuilds: number
  error?: Error | null
  isLoading: boolean
  onlineRunners: number
  noOnlineRunners: boolean
  onRetry: () => void
  recentBuilds: Array<Build>
  runnersError: boolean
  runnersLoading: boolean
  runningBuilds: number
  statusCountsError: boolean
  statusCountsLoading: boolean
  successfulBuilds: number
  totalRunners: number
  waitingBuilds: number
}) {
  const blockedBuildIds = new Set(blockedBuilds.map((build) => build.id))
  const activityBuilds = [
    ...activeBuilds
      .filter((build) => !blockedBuildIds.has(build.id))
      .sort(
        (left, right) =>
          ACTIVITY_STATUS_PRIORITY[left.status] -
            ACTIVITY_STATUS_PRIORITY[right.status] ||
          right.updated_at - left.updated_at,
      ),
    ...recentBuilds,
  ].slice(0, 8)
  return (
    <div className="flex flex-col gap-8">
      <DashboardSystemStatus
        buildsError={statusCountsError}
        buildsLoading={statusCountsLoading}
        completedBuilds={completedBuilds}
        onlineRunners={onlineRunners}
        recentBuildsError={!!error}
        recentBuildsLoading={isLoading}
        runnersError={runnersError}
        runnersLoading={runnersLoading}
        runningBuilds={runningBuilds}
        successfulBuilds={successfulBuilds}
        totalRunners={totalRunners}
        waitingBuilds={waitingBuilds}
      />

      {!isLoading ? (
        <DashboardBuildIncident
          builds={blockedBuilds}
          noOnlineRunners={noOnlineRunners}
        />
      ) : null}

      <section className="flex flex-col gap-3" aria-labelledby="build-activity">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2
            id="build-activity"
            className="text-sm font-medium text-muted-foreground"
          >
            Build activity
          </h2>
          <Button
            variant="ghost"
            size="sm"
            render={<Link to="/builds" />}
            nativeButton={false}
          >
            View all
            <HugeiconsIcon icon={ChevronRightIcon} data-icon="inline-end" />
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
          <div className="flex flex-col gap-2">
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
          </div>
        ) : activityBuilds.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <HugeiconsIcon icon={CheckCircleIcon} />
              </EmptyMedia>
              <EmptyTitle>No build activity</EmptyTitle>
              <EmptyDescription>
                Run a build to see its status here.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <ItemGroup className="gap-2">
            {activityBuilds.map((build) => (
              <BuildItem key={build.id} build={build} />
            ))}
          </ItemGroup>
        )}
      </section>
    </div>
  )
}
