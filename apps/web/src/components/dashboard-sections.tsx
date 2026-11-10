import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Link04Icon, Add01Icon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import type { Runner, RuntimeMode } from '@oore/client/models'
import { SampleAppButton } from '@/components/sample-app-button'
import { RunnerToolchainCell } from '@/components/runner-toolchain'
import { StatusBadge } from '@/components/status-badge'
import { useRunners } from '@/hooks/use-runners'
import { Row } from '@/components/layout/row'
import { useFirstAppScope, useFirstAppStore } from '@/stores/first-app-store'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'

const DEMO_URL = 'https://demo.oore.build'

function runnerActivity(runner: Runner) {
  if (runner.status === 'offline') return 'Offline'
  if (runner.status === 'busy') return 'Building now'
  if (runner.status === 'draining') return 'Finishing its builds'
  return 'Online and waiting for a build'
}

/**
 * Home on a new instance: add the first app, what the runner is doing and
 * can build, and the demo, with nothing about health that has no data yet.
 */
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
  const runnersQuery = useRunners({ limit: 5 })
  const runners = runnersQuery.data?.runners ?? []
  const needsSource = runtimeMode === 'remote' && noConnectedSources

  const addApp = !canWriteProjects ? (
    <Text variant="muted">Ask an owner or admin to add the first app.</Text>
  ) : needsSource ? (
    canWriteIntegrations ? (
      <Button
        onClick={() => updateProgress(scope, { projectDraft: { name: '' } })}
        render={<Link to="/settings/sources" />}
        nativeButton={false}
      >
        <HugeiconsIcon icon={Link04Icon} data-icon="inline-start" />
        Connect GitHub or GitLab
      </Button>
    ) : (
      <Text variant="muted">
        Ask an admin to connect GitHub or GitLab first.
      </Text>
    )
  ) : (
    <Button
      render={<Link to="/projects" search={{ openCreate: 1 }} />}
      nativeButton={false}
    >
      <HugeiconsIcon icon={Add01Icon} data-icon="inline-start" />
      Add your app
    </Button>
  )

  return (
    <div className="grid grid-cols-1 gap-4 @3xl/main:grid-cols-3">
      <Card size="sm" className="@3xl/main:col-span-2">
        <CardHeader>
          <CardTitle>
            <h2>Add your app</h2>
          </CardTitle>
          <CardDescription>
            {needsSource
              ? 'Connect where your code lives, then pick the repository. Oore reads it, proposes what to build and starts the first build.'
              : runtimeMode === 'local'
                ? 'Point Oore at a Flutter repository on this Mac. It reads it, proposes what to build and starts the first build.'
                : 'Pick a repository. Oore reads it, proposes what to build and starts the first build.'}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Row gap={2} className="flex-wrap">
            {addApp}
            {canWriteProjects && !needsSource ? <SampleAppButton /> : null}
            <Button
              variant="ghost"
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
          </Row>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle>
            <h2>Your runner</h2>
          </CardTitle>
          <CardAction>
            <Button
              variant="ghost"
              size="sm"
              className="-me-2"
              render={<Link to="/settings/runners" />}
              nativeButton={false}
            >
              Runners
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          {runnersQuery.isPending ? (
            <Skeleton className="h-12 w-full" />
          ) : runners.length === 0 ? (
            <Text variant="muted">
              No runner yet. Builds run on a Mac: register this Mac or another
              one under Runners.
            </Text>
          ) : (
            <Stack gap={3} render={<ul />}>
              {runners.map((runner) => (
                <Stack gap={1} key={runner.id} render={<li />}>
                  <Row gap={2} className="justify-between">
                    <span className="truncate text-sm font-medium">
                      {runner.name}
                    </span>
                    <StatusBadge kind="runner" value={runner.status} />
                  </Row>
                  <Text variant="caption">{runnerActivity(runner)}</Text>
                  <RunnerToolchainCell runner={runner} />
                </Stack>
              ))}
            </Stack>
          )}
        </CardContent>
      </Card>

      <Card size="sm" className="@3xl/main:col-span-3">
        <CardHeader>
          <CardTitle>
            <h2>See it with sample apps first</h2>
          </CardTitle>
          <CardDescription>
            The demo shows Oore with apps, builds, releases and testers already
            in it. Nothing you do there touches this instance.
          </CardDescription>
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              render={
                <a href={DEMO_URL} target="_blank" rel="noopener noreferrer" />
              }
              nativeButton={false}
            >
              Open the demo
            </Button>
          </CardAction>
        </CardHeader>
      </Card>
    </div>
  )
}
