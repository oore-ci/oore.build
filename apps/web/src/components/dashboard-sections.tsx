import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  CheckmarkCircle02Icon as CheckCircleIcon,
  Link04Icon,
  Add01Icon,
} from '@hugeicons/core-free-icons'

import { BuildItem } from '@/components/build-item'
import {
  DashboardList,
  DashboardSection,
  DashboardViewAll,
} from '@/components/dashboard-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Skeleton } from '@/components/ui/skeleton'
import type { RuntimeMode } from '@oore/client/models'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import type { Build } from '@oore/client/models'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

type GettingStartedStep = {
  key: string
  title: string
  hint: string
  action?: React.ReactNode
}

export function DashboardGettingStarted({
  canWriteIntegrations,
  canWriteProjects,
  noConnectedSources,
  noOnlineRunners,
  runtimeMode,
}: {
  canWriteIntegrations: boolean
  canWriteProjects: boolean
  noConnectedSources: boolean
  noOnlineRunners: boolean
  runtimeMode: RuntimeMode
}) {
  const scope = useFirstAppScope()
  const updateProgress = useFirstAppStore((state) => state.update)
  const hasSourceStep = runtimeMode === 'remote' && noConnectedSources
  const canCreateProject = canWriteProjects && !noConnectedSources

  const steps: Array<GettingStartedStep> = [
    ...(hasSourceStep
      ? [
          {
            key: 'source',
            title: 'Connect a source',
            hint: canWriteIntegrations
              ? 'Link GitHub or GitLab to import repositories and build on every push.'
              : 'Ask an admin to connect GitHub or GitLab.',
            action: canWriteIntegrations ? (
              <Button
                size="sm"
                onClick={() =>
                  updateProgress(scope, { projectDraft: { name: '' } })
                }
                render={<Link to="/settings/integrations" />}
                nativeButton={false}
              >
                <HugeiconsIcon icon={Link04Icon} data-icon="inline-start" />
                Connect source
              </Button>
            ) : undefined,
          },
        ]
      : []),
    {
      key: 'project',
      title: 'Create a project',
      hint: !canWriteProjects
        ? 'Ask an owner or admin to create a project.'
        : noConnectedSources
          ? 'Connect a source first, then pick one of its repositories.'
          : runtimeMode === 'local'
            ? 'Point to a local Flutter repository.'
            : 'Pick a repository from a connected source.',
      action: canCreateProject ? (
        <Button
          size="sm"
          render={<Link to="/projects" search={{ openCreate: 1 }} />}
          nativeButton={false}
        >
          <HugeiconsIcon icon={Add01Icon} data-icon="inline-start" />
          Create project
        </Button>
      ) : undefined,
    },
    ...(noOnlineRunners
      ? [
          {
            key: 'runner',
            title: 'Connect a runner',
            hint: 'Builds run on a Mac runner. Register this Mac or another one.',
            action: (
              <Button
                size="sm"
                variant="outline"
                render={<Link to="/settings/runners" />}
                nativeButton={false}
              >
                Open Runners
              </Button>
            ),
          },
        ]
      : []),
    {
      key: 'pipeline',
      title: 'Add a pipeline',
      hint: 'Choose the platforms to build and how to sign them.',
    },
    {
      key: 'run',
      title: 'Run and install your app',
      hint: 'Run a build, then install the app or share it with a tester.',
    },
  ]

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>
          <h2>Build and share your first app</h2>
        </CardTitle>
        <CardAction>
          <Button
            variant="ghost"
            size="sm"
            className="-me-2"
            render={
              <a
                href="https://docs.oore.build/start/install"
                target="_blank"
                rel="noopener noreferrer"
              />
            }
            nativeButton={false}
          >
            Setup guide
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <Stack gap={4} render={<ol />} className="text-sm">
          {steps.map((step, index) => (
            <li key={step.key} className="flex items-start gap-3">
              <Badge variant="outline" className="mt-0.5 size-5 px-0">
                {index + 1}
              </Badge>
              <Stack gap={1.5} className="min-w-0 items-start">
                <p className="font-medium">{step.title}</p>
                <Text variant="caption">{step.hint}</Text>
                {step.action}
              </Stack>
            </li>
          ))}
        </Stack>
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

export function DashboardBuildActivity({
  activeBuilds,
  blockedBuilds,
  error,
  isLoading,
  onRetry,
  recentBuilds,
}: {
  activeBuilds: Array<Build>
  blockedBuilds: Array<Build>
  error?: Error | null
  isLoading: boolean
  onRetry: () => void
  recentBuilds: Array<Build>
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
  ].slice(0, 6)
  return (
    <DashboardSection
      title="Build activity"
      action={<DashboardViewAll to="/builds" />}
    >
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
        <Skeleton className="h-48 w-full" />
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
        <DashboardList>
          {activityBuilds.map((build) => ({
            key: build.id,
            node: <BuildItem build={build} inList />,
          }))}
        </DashboardList>
      )}
    </DashboardSection>
  )
}
