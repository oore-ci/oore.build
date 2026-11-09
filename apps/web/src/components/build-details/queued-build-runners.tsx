import * as z from 'zod'
import { Link } from '@tanstack/react-router'
import type { Build, Runner } from '@oore/client/models'

import { StatusBadge } from '@/components/status-badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { useBuilds } from '@/hooks/use-builds'
import { useHasPermission } from '@/hooks/use-permissions'
import { useRunners } from '@/hooks/use-runners'
import { relativeTime } from '@/lib/format-utils'

/** The labels a build needs its runner to have, from its pipeline. */
export function buildRunsOn(build: Pick<Build, 'config_snapshot'>) {
  const runsOn = z.array(z.string()).safeParse(build.config_snapshot.runs_on)
  return runsOn.success ? runsOn.data : []
}

/** Why a runner that fits has not picked the build up, in one phrase. */
export function runnerAvailability(
  runner: Pick<
    Runner,
    'status' | 'last_heartbeat_at' | 'max_concurrent_builds'
  >,
  running: ReadonlyArray<Pick<Build, 'build_number'>>,
): string {
  if (runner.status === 'offline') {
    return runner.last_heartbeat_at
      ? `Offline since ${relativeTime(runner.last_heartbeat_at)}`
      : 'Offline'
  }
  if (runner.status === 'draining') return 'Finishing its builds, then stops'
  if (
    runner.status === 'busy' ||
    running.length >= (runner.max_concurrent_builds ?? 1)
  ) {
    return running.length
      ? `Busy with ${running.map((build) => `#${build.build_number}`).join(', ')}`
      : 'Busy with other builds'
  }
  return 'Free, should pick it up within a few seconds'
}

/**
 * For a build no runner has started: which runners could take it and why
 * they have not yet. Needs permission to see runners.
 */
export function QueuedBuildRunners({ build }: { build: Build }) {
  const canReadRunners = useHasPermission('runners:read')
  const runnersQuery = useRunners({ limit: 100 }, { enabled: canReadRunners })
  const runningQuery = useBuilds(
    { status: 'running', limit: 100 },
    { enabled: canReadRunners },
  )
  if (!canReadRunners || !runnersQuery.data) return null

  const runsOn = buildRunsOn(build)
  const runners = runnersQuery.data.runners
  // Free runners first, then busy, finishing and offline ones.
  const order = { online: 0, busy: 1, draining: 2, offline: 3 }
  const fitting = runners
    .filter((runner) => runsOn.every((label) => runner.labels?.includes(label)))
    .sort((left, right) => order[left.status] - order[right.status])
  const misfits = runners.length - fitting.length
  const runningBy = (runnerId: string) =>
    (runningQuery.data?.builds ?? []).filter(
      (running) => running.runner_id === runnerId,
    )

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Runners that can take it</CardTitle>
      </CardHeader>
      <CardContent>
        <Stack gap={3}>
          {fitting.length === 0 ? (
            <Text variant="muted">
              No runner has every label this pipeline needs
              {runsOn.length ? ` (${runsOn.join(', ')})` : ''}. Add the labels
              to a runner on{' '}
              <Link
                to="/settings/runners"
                className="underline underline-offset-4"
              >
                Runners
              </Link>
              .
            </Text>
          ) : (
            <ul className="divide-y">
              {fitting.map((runner) => {
                const running = runningBy(runner.id)
                return (
                  <li key={runner.id} className="py-2">
                    <Row gap={3} className="justify-between">
                      <div className="min-w-0">
                        <p className="truncate font-medium">{runner.name}</p>
                        <Text variant="caption">
                          {runnerAvailability(runner, running)}
                        </Text>
                      </div>
                      <StatusBadge kind="runner" value={runner.status} />
                    </Row>
                  </li>
                )
              })}
            </ul>
          )}
          {misfits > 0 && runsOn.length ? (
            <Text variant="caption">
              {misfits === 1
                ? '1 other runner lacks'
                : `${misfits} others lack`}{' '}
              the labels this pipeline needs: {runsOn.join(', ')}.
            </Text>
          ) : null}
        </Stack>
      </CardContent>
    </Card>
  )
}
