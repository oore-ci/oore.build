import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import type {
  InboxItem,
  NotificationPreferences,
  ReleaseState,
  WeeklyDigest,
} from '@oore/client/models'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { demoState } from '../state'
import { ago } from '../seed'

const DAY = 86_400
const RELEASE_WORDS = new Map<ReleaseState, string>([
  ['blocked', 'was refused by the store'],
  ['in_review', 'is in review'],
  ['approved', 'was approved'],
  ['rejected', 'was rejected'],
  ['rolling', 'is rolling out'],
  ['paused', 'was paused'],
  ['halted', 'was halted'],
  ['complete', 'is live'],
  ['failed', 'failed'],
])

function releasePlace(store: string, track: string) {
  if (store !== 'app_store') return `Google Play · ${track}`
  return track === 'testflight' ? 'TestFlight' : 'App Store'
}

const preferencesByUser = new Map<string, NotificationPreferences>()

function preferencesFor(userId: string): NotificationPreferences {
  return (
    preferencesByUser.get(userId) ?? {
      my_builds: 'failed',
      approvals: true,
      followed_releases: true,
      email: false,
      browser: true,
      weekly_digest: false,
    }
  )
}

function projectName(projectId: string) {
  return (
    demoState.projects.find((project) => project.id === projectId)?.name ??
    'App'
  )
}

function inboxFor(request: Request): Array<InboxItem> {
  const persona = getDemoPersonaFromRequest(request)
  const since = ago(7 * DAY)
  const items: Array<InboxItem> = []
  for (const build of demoState.builds) {
    const role = getDemoProjectRole(persona, build.project_id)
    if (!role) continue
    if (build.approval?.state === 'pending' && role === 'maintainer') {
      items.push({
        id: `approval:${build.id}`,
        kind: 'approval',
        title: `Build #${build.build_number} waits for your approval`,
        detail: build.branch ?? '',
        project_id: build.project_id,
        project_name: projectName(build.project_id),
        build_id: build.id,
        at: build.approval.requested_at ?? build.created_at,
      })
    }
    if (
      (build.status === 'failed' || build.status === 'timed_out') &&
      build.trigger_actor === persona.email &&
      (build.finished_at ?? build.updated_at) >= since
    ) {
      items.push({
        id: `failed:${build.id}`,
        kind: 'failed_build',
        title: `Your build #${build.build_number} failed`,
        detail: build.branch ?? '',
        project_id: build.project_id,
        project_name: projectName(build.project_id),
        build_id: build.id,
        at: build.finished_at ?? build.updated_at,
      })
    }
  }
  const pinned = new Set(demoState.favoriteProjects[persona.userId] ?? [])
  for (const release of demoState.releases) {
    if (!pinned.has(release.project_id)) continue
    for (const entry of demoState.releaseHistory[release.id] ?? []) {
      const words = RELEASE_WORDS.get(entry.to_state)
      if (entry.at < since || !words) continue
      const name = projectName(release.project_id)
      items.push({
        id: `release:${entry.id}`,
        kind: 'release',
        title: `${name} ${release.version_name ?? 'release'} ${words}`,
        detail: releasePlace(release.store, release.track),
        project_id: release.project_id,
        project_name: name,
        build_id: release.build_id ?? undefined,
        release_id: release.id,
        at: entry.at,
      })
    }
  }
  return items.sort((left, right) => right.at - left.at).slice(0, 50)
}

function digestFor(request: Request): WeeklyDigest {
  const persona = getDemoPersonaFromRequest(request)
  const since = ago(7 * DAY)
  const pinned = new Set(demoState.favoriteProjects[persona.userId] ?? [])
  const apps = demoState.projects
    .filter(
      (project) =>
        getDemoProjectRole(persona, project.id) &&
        (pinned.size === 0 || pinned.has(project.id)),
    )
    .map((project) => {
      const builds = demoState.builds.filter(
        (build) =>
          build.project_id === project.id && (build.finished_at ?? 0) >= since,
      )
      return {
        project_id: project.id,
        name: project.name,
        builds: builds.length,
        succeeded: builds.filter((build) => build.status === 'succeeded')
          .length,
        failed: builds.filter((build) => build.status === 'failed').length,
        slowest_pipeline: { name: 'iOS release', median_seconds: 754 },
        releases: demoState.releases
          .filter(
            (release) =>
              release.project_id === project.id && release.updated_at >= since,
          )
          .map((release) => ({
            version: release.version_name ?? '',
            store: release.store,
            track: release.track,
            state: release.state,
          })),
      }
    })
    .filter((app) => app.builds > 0 || app.releases.length > 0)
  return {
    period_start: since,
    period_end: ago(0),
    apps,
    expiring: [
      {
        name: 'Profile FlutterShop App Store',
        project_name: 'FlutterShop',
        expires_at: ago(-12 * DAY),
      },
    ],
  }
}

export const personalHandlers = [
  demoApi.getNotificationPreferences(async ({ request }) => {
    await delay(100)
    const persona = getDemoPersonaFromRequest(request)
    return HttpResponse.json({
      preferences: preferencesFor(persona.userId),
      email_available: demoState.notificationChannels.some(
        (channel) => channel.channel_type === 'email' && channel.enabled,
      ),
      email_address: persona.email,
    })
  }),

  demoApi.updateNotificationPreferences(async ({ request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const body: NotificationPreferences = await request.json()
    preferencesByUser.set(persona.userId, body)
    return HttpResponse.json({
      preferences: body,
      email_available: demoState.notificationChannels.some(
        (channel) => channel.channel_type === 'email' && channel.enabled,
      ),
      email_address: persona.email,
    })
  }),

  demoApi.getInbox(async ({ request }) => {
    await delay(120)
    return HttpResponse.json({ items: inboxFor(request) })
  }),

  demoApi.getWeeklyDigest(async ({ request }) => {
    await delay(150)
    return HttpResponse.json(digestFor(request))
  }),

  demoApi.sendWeeklyDigest(async () => {
    await delay(300)
    return new HttpResponse(null, { status: 202 })
  }),

  demoApi.addProjectFavorite(async ({ params, request }) => {
    await delay(100)
    const persona = getDemoPersonaFromRequest(request)
    const projectId = String(params.project_id)
    const pinned = demoState.favoriteProjects[persona.userId] ?? []
    if (!pinned.includes(projectId)) pinned.push(projectId)
    demoState.favoriteProjects[persona.userId] = pinned
    return new HttpResponse(null, { status: 204 })
  }),

  demoApi.removeProjectFavorite(async ({ params, request }) => {
    await delay(100)
    const persona = getDemoPersonaFromRequest(request)
    demoState.favoriteProjects[persona.userId] = (
      demoState.favoriteProjects[persona.userId] ?? []
    ).filter((id) => id !== String(params.project_id))
    return new HttpResponse(null, { status: 204 })
  }),
]
