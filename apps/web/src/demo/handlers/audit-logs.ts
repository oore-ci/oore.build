import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import type { AuditLogCategory, AuditLogEntry } from '@oore/client/models'
import { demoState } from '../state'
import { parseAuditDetails } from '@/lib/audit-log'

const DAY = 86_400

const ACCESS_RESOURCE_TYPES = new Set([
  'auth',
  'user',
  'api_token',
  'local_recovery_capability',
  'local_operator_request',
])
const SYSTEM_ACTIONS = new Set([
  'retention_cleanup_completed',
  'artifact_expiry_cleanup_completed',
])
const BUILD_RESOURCE_TYPES = new Set([
  'build',
  'release',
  'artifact',
  'artifact_download_token',
  'environment_promotion',
])
const BUILD_ACTIONS = new Set([
  'release_freeze_set',
  'release_freeze_lifted',
  'release_notes_updated',
  'latest_install_link_created',
  'latest_install_link_deleted',
  'ios_device_self_registered',
  'pipeline_ios_device_registered',
])

// The same rule as the daemon's `audit_category`.
function category(entry: AuditLogEntry): AuditLogCategory {
  if (
    ACCESS_RESOURCE_TYPES.has(entry.resource_type) ||
    entry.action.startsWith('project_member_')
  ) {
    return 'access'
  }
  if (SYSTEM_ACTIONS.has(entry.action)) return 'system'
  if (
    BUILD_RESOURCE_TYPES.has(entry.resource_type) ||
    BUILD_ACTIONS.has(entry.action)
  ) {
    return 'builds'
  }
  return 'settings'
}

function referencedIds(entry: AuditLogEntry) {
  const ids = entry.resource_id ? [entry.resource_id] : []
  for (const value of parseAuditDetails(entry.details).fields.values()) {
    if (value.kind === 'text' && !ids.includes(value.text)) {
      ids.push(value.text)
    }
  }
  return ids
}

/** Names the records an entry refers to, as the daemon does on read. */
function withNames(entry: AuditLogEntry): AuditLogEntry {
  const ids = referencedIds(entry)
  const artifacts = Object.entries(demoState.artifacts).flatMap(
    ([buildId, list]) =>
      (list ?? []).map((artifact) => ({ artifact, buildId })),
  )
  const buildOf = (id: string) =>
    demoState.builds.find((build) => build.id === id) ??
    demoState.builds.find(
      (build) =>
        build.id ===
        artifacts.find(({ artifact }) => artifact.id === id)?.buildId,
    )
  const build = ids.map(buildOf).find(Boolean)
  const pipeline = demoState.pipelines.find(
    (candidate) =>
      candidate.id ===
      (build?.pipeline_id ??
        ids.find((id) => demoState.pipelines.some((p) => p.id === id))),
  )
  const projectId =
    build?.project_id ??
    pipeline?.project_id ??
    ids.find((id) => demoState.projects.some((project) => project.id === id))
  const project = demoState.projects.find(
    (candidate) => candidate.id === projectId,
  )

  const known = new Map<string, string>([
    ...demoState.users.map(
      (user) => [user.id, user.display_name || user.email] as const,
    ),
    ...demoState.projects.map((item) => [item.id, item.name] as const),
    ...demoState.pipelines.map((item) => [item.id, item.name] as const),
    ...demoState.runners.map((item) => [item.id, item.name] as const),
    ...demoState.integrations.map(
      (item) => [item.id, item.display_name ?? item.provider] as const,
    ),
    ...demoState.notificationChannels.map(
      (item) => [item.id, item.name] as const,
    ),
    ...demoState.apiTokens.map((item) => [item.id, item.name] as const),
    ...artifacts.map(({ artifact }) => [artifact.id, artifact.name] as const),
    ['grp-demo-qa', 'QA team'],
  ])
  const actor = demoState.users.find((user) => user.id === entry.actor_id)

  return {
    ...entry,
    actor_name: actor?.display_name ?? null,
    category: category(entry),
    project_id: project?.id ?? null,
    project_name: project?.name ?? null,
    pipeline_id: pipeline?.id ?? null,
    pipeline_name: pipeline?.name ?? null,
    build_id: build?.id ?? null,
    build_number: build?.build_number ?? null,
    names: Object.fromEntries(
      ids.flatMap((id) => (known.has(id) ? [[id, known.get(id)!]] : [])),
    ),
  }
}

function matchesSearch(entry: AuditLogEntry, query: string) {
  const needle = query.toLowerCase()
  return [
    entry.action,
    entry.action.replaceAll('_', ' '),
    entry.actor_email,
    entry.actor_name,
    entry.details,
    entry.resource_id,
    entry.project_name,
    entry.pipeline_name,
  ].some((value) => value?.toLowerCase().includes(needle))
}

export const auditLogHandlers = [
  demoApi.listAuditLogs(async ({ request }) => {
    await delay(200)
    const params = new URL(request.url).searchParams

    const limit = Math.min(Number(params.get('limit')) || 50, 200)
    const offset = Number(params.get('offset')) || 0
    const actorId = params.get('actor_id')
    const query = params.get('q')?.trim()
    const kind = params.get('category')
    const fromTs = Number(params.get('from_ts')) || undefined
    const toTs = Number(params.get('to_ts')) || undefined

    const filtered = demoState.auditLogs
      .map(withNames)
      .filter(
        (entry) =>
          (!actorId ||
            (actorId === 'system'
              ? !entry.actor_id
              : entry.actor_id === actorId)) &&
          (!kind || entry.category === kind) &&
          (!query || matchesSearch(entry, query)) &&
          (!fromTs || entry.created_at >= fromTs) &&
          (!toTs || entry.created_at <= toTs),
      )
      .sort(
        (left, right) =>
          right.created_at - left.created_at || right.id - left.id,
      )

    return HttpResponse.json({
      entries: filtered.slice(offset, offset + limit),
      total: filtered.length,
    })
  }),

  demoApi.getAuditLogActivity(async ({ request }) => {
    await delay(150)
    const params = new URL(request.url).searchParams
    const span = Math.min(Number(params.get('days')) || 14, 90)
    const offset = (Number(params.get('utc_offset_minutes')) || 0) * 60
    const dayOf = (epochSecs: number) => Math.floor((epochSecs + offset) / DAY)
    const firstDay = dayOf(Date.now() / 1000) - (span - 1)
    const recent = demoState.auditLogs.filter(
      (entry) => dayOf(entry.created_at) >= firstDay,
    )

    return HttpResponse.json({
      days: Array.from({ length: span }, (_, index) => ({
        date: new Date((firstDay + index) * DAY * 1000)
          .toISOString()
          .slice(0, 10),
        count: recent.filter(
          (entry) => dayOf(entry.created_at) === firstDay + index,
        ).length,
      })),
      total: recent.length,
      actors: new Set(recent.flatMap((entry) => entry.actor_id ?? [])).size,
      problems: recent.filter((entry) =>
        /blocked|failed|not_allowed/.test(entry.action),
      ).length,
    })
  }),
]
