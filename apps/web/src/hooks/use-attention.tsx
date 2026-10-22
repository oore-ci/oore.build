import { Link } from '@tanstack/react-router'

import type {
  Build,
  FailingPipelineInsights,
  ListBuildsResponse,
  ListRunnersResponse,
  OperatorIncident,
  RunnerPolicyBlockReason,
} from '@oore/client/models'
import {
  incidentLinkProps,
  incidentTitle,
} from '@/components/operator-incident-alert'
import { useBuildInsights, useBuilds } from '@/hooks/use-builds'
import {
  useMarkOperatorIncidentRead,
  useOperatorIncidents,
} from '@/hooks/use-operator-incidents'
import { useHasPermission } from '@/hooks/use-permissions'
import { useRunners } from '@/hooks/use-runners'
import { relativeTime } from '@/lib/format-utils'
import { getRunnerPolicyBlockLabel } from '@/lib/status-variants'

/**
 * The widest Build health window. A pipeline failing in any shorter window is
 * failing here too, so Needs attention never misses one that Build health
 * marks as failing.
 */
export const ATTENTION_INSIGHTS_DAYS = 90

/** Blocking items stop builds; failing ones are broken builds; warnings can wait. */
export type AttentionSeverity = 'blocking' | 'failing' | 'warning'

export type AttentionItem = {
  key: string
  severity: AttentionSeverity
  title: string
  description: string
  /** A router `Link` without children; the list decides how to render it. */
  link: React.ReactElement
  onSelect?: () => void
}

const SEVERITY_ORDER = {
  blocking: 0,
  failing: 1,
  warning: 2,
} satisfies Record<AttentionSeverity, number>

const ACTIVE_STATUSES = new Set<Build['status']>([
  'queued',
  'scheduled',
  'assigned',
  'running',
])

function selectBlockedBuilds({ builds }: ListBuildsResponse) {
  return builds.filter(
    (build) =>
      ACTIVE_STATUSES.has(build.status) && build.runner_policy_block_reason,
  )
}

function selectNoRunnerOnline({ online_total }: ListRunnersResponse) {
  return online_total === 0
}

function blockDescription(reason: RunnerPolicyBlockReason): string {
  if (reason === 'repository_unavailable')
    return "Oore can't check out the source"
  return getRunnerPolicyBlockLabel(reason)
}

function waitingCount(count: number): string {
  return count === 1 ? '1 build waiting' : `${count} builds waiting`
}

function blockedItems(blocked: Array<Build>): Array<AttentionItem> {
  const oldestFirst = [...blocked].sort((a, b) => a.queued_at - b.queued_at)
  const items: Array<AttentionItem> = []

  // An instance-wide pause is one cause with one fix, not one item per project.
  const paused = oldestFirst.filter(
    (build) => build.runner_policy_block_reason === 'instance_paused',
  )
  if (paused.length > 0) {
    items.push({
      key: 'instance-paused',
      severity: 'blocking',
      title: 'Mac builds are paused',
      description: `${waitingCount(paused.length)} since ${relativeTime(paused[0].queued_at)} · Resume in Runners`,
      link: <Link to="/settings/runners" />,
    })
  }

  const byProject = new Map<string, Array<Build>>()
  for (const build of oldestFirst) {
    if (build.runner_policy_block_reason === 'instance_paused') continue
    byProject.set(build.project_id, [
      ...(byProject.get(build.project_id) ?? []),
      build,
    ])
  }
  for (const builds of byProject.values()) {
    const oldest = builds[0]
    const reason = oldest.runner_policy_block_reason
    items.push({
      key: `blocked-${oldest.project_id}`,
      severity: 'blocking',
      title: `${oldest.context?.project_name ?? oldest.project_id} builds are blocked`,
      description: `${reason ? blockDescription(reason) : 'Waiting'} · ${waitingCount(builds.length)} since ${relativeTime(oldest.queued_at)}`,
      link: <Link to="/builds/$buildId" params={{ buildId: oldest.id }} />,
    })
  }
  return items
}

function streakText(pipeline: FailingPipelineInsights): string {
  return pipeline.current_streak > 1
    ? `failed ${pipeline.current_streak} in a row`
    : 'last build failed'
}

/** One item per project whose newest verdict, in some pipeline, is a failure. */
function failingItems(
  pipelines: Array<FailingPipelineInsights>,
): Array<AttentionItem> {
  const byProject = new Map<string, Array<FailingPipelineInsights>>()
  for (const pipeline of pipelines) {
    if (pipeline.current_streak === 0) continue
    byProject.set(pipeline.project_id, [
      ...(byProject.get(pipeline.project_id) ?? []),
      pipeline,
    ])
  }
  return [...byProject.values()].map((failing) => {
    // The server ranks the longest streak first.
    const worst = failing[0]
    const where =
      failing.length === 1
        ? `${worst.pipeline_name} ${streakText(worst)}`
        : `${failing.length} pipelines failing`
    return {
      key: `failing-${worst.project_id}`,
      severity: 'failing',
      title: `${worst.project_name} builds are failing`,
      description: `${where} · #${worst.last_failed_build_number} ${relativeTime(worst.last_failed_at)}`,
      link: (
        <Link
          to="/builds/$buildId"
          params={{ buildId: worst.last_failed_build_id }}
        />
      ),
    }
  })
}

function incidentItem(
  incident: OperatorIncident,
  onRead: (id: string) => void,
): AttentionItem {
  return {
    key: `incident-${incident.id}`,
    severity: incident.severity === 'critical' ? 'blocking' : 'warning',
    title: incidentTitle(incident),
    description: incident.repair_action,
    link: <Link {...incidentLinkProps(incident)} />,
    onSelect: () => onRead(incident.id),
  }
}

/**
 * Everything that needs someone now, from one source for Home's Needs
 * attention and the header bell, so the two never disagree.
 */
export function useAttention() {
  const canRepair = useHasPermission('integrations:write')
  const canReadRunners = useHasPermission('runners:read')
  const incidentsQuery = useOperatorIncidents({ enabled: canRepair })
  const markRead = useMarkOperatorIncidentRead()
  const insightsQuery = useBuildInsights({ days: ATTENTION_INSIGHTS_DAYS })
  const blockedQuery = useBuilds({ limit: 50 }, { select: selectBlockedBuilds })
  const runnersQuery = useRunners(
    { limit: 1 },
    { enabled: canReadRunners, select: selectNoRunnerOnline },
  )

  const items: Array<AttentionItem> = [
    ...(runnersQuery.data
      ? [
          {
            key: 'no-runner',
            severity: 'blocking',
            title: 'No runner is online',
            description: 'Builds wait until a runner checks in',
            link: <Link to="/settings/runners" />,
          } satisfies AttentionItem,
        ]
      : []),
    ...blockedItems(blockedQuery.data ?? []),
    ...(incidentsQuery.data?.incidents ?? []).map((incident) =>
      incidentItem(incident, (id) => markRead.mutate(id)),
    ),
    ...failingItems(insightsQuery.data?.failing_pipelines ?? []),
  ].sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity])

  const queries = [insightsQuery, blockedQuery, runnersQuery, incidentsQuery]
  return {
    items,
    /** Still waiting on a source; the list may be incomplete. */
    isLoading: queries.some((query) => query.isLoading),
    /** A source failed, so an empty list does not mean all clear. */
    isError: queries.some((query) => query.isError && !query.data),
    refetch: () => {
      for (const query of queries) {
        if (query.isError) void query.refetch()
      }
    },
    unreadIncidents: (incidentsQuery.data?.incidents ?? []).filter(
      (incident) => !incident.read_at,
    ).length,
  }
}
