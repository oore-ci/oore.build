import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import type {
  ProjectRole,
  ReleaseAction,
  ReleaseActionRequest,
  ReleaseState,
} from '@oore/client/models'
import { ago } from '../seed'
import type { DemoRelease } from '../data/releases'
import { demoStoreVitals } from '../data/store-vitals'
import type { DemoPersona } from '../personas'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { demoState } from '../state'

const releaseActionRequestSchema = z.toZod<ReleaseActionRequest>()(
  z.object({
    reason: z.string().nullish(),
    fraction: z.number().nullish(),
  }),
)

// Mirrors the daemon: developers can stop a rollout, maintainers drive it.
const ROLE_ACTIONS = {
  maintainer: new Set<ReleaseAction>([
    'approve',
    'raise',
    'pause',
    'resume',
    'halt',
    'complete',
  ]),
  developer: new Set<ReleaseAction>(['pause', 'halt']),
  viewer: new Set<ReleaseAction>(),
} satisfies Record<ProjectRole, ReadonlySet<ReleaseAction>>

function stateActions(release: DemoRelease): Array<ReleaseAction> {
  if (release.awaiting_approval) return ['approve']
  switch (release.state) {
    case 'rolling':
      return ['raise', 'pause', 'halt', 'complete']
    case 'paused':
      return ['resume', 'halt', 'complete']
    case 'halted':
      return ['resume']
    case 'uploading':
    case 'processing':
    case 'in_review':
      return ['halt']
    case 'complete':
    case 'failed':
      return []
  }
}

function withReleaseContext(release: DemoRelease, persona: DemoPersona) {
  const role = getDemoProjectRole(persona, release.project_id)
  const allowed = role ? ROLE_ACTIONS[role] : new Set<ReleaseAction>()
  return {
    ...release,
    project_name:
      demoState.projects.find((project) => project.id === release.project_id)
        ?.name ?? 'Unknown app',
    allowed_actions: stateActions(release).filter((action) =>
      allowed.has(action),
    ),
  }
}

function notFound() {
  return HttpResponse.json(
    { error: 'Release not found', code: 'not_found' },
    { status: 404 },
  )
}

function nextFraction(release: DemoRelease): number | null {
  return (
    (release.policy.steps ?? []).find(
      (step) => step > release.rollout_fraction,
    ) ?? null
  )
}

interface ReleaseTransition {
  state: ReleaseState
  fraction: number
}

function applyAction(
  release: DemoRelease,
  action: ReleaseAction,
  fraction: number | null | undefined,
): ReleaseTransition {
  switch (action) {
    case 'approve':
      return { state: release.state, fraction: release.rollout_fraction }
    case 'raise':
      return {
        state: 'rolling',
        fraction: fraction ?? nextFraction(release) ?? 1,
      }
    case 'pause':
      return { state: 'paused', fraction: release.rollout_fraction }
    case 'resume':
      return { state: 'rolling', fraction: release.rollout_fraction }
    case 'halt':
      return { state: 'halted', fraction: release.rollout_fraction }
    case 'complete':
      return { state: 'complete', fraction: 1 }
  }
}

function releaseActionHandler(action: ReleaseAction) {
  return async ({
    params,
    request,
  }: {
    params: Record<string, string | ReadonlyArray<string> | undefined>
    request: Request
  }) => {
    await delay(250)
    const persona = getDemoPersonaFromRequest(request)
    const release = demoState.releases.find(
      (candidate) => candidate.id === params.release_id,
    )
    if (!release || !getDemoProjectRole(persona, release.project_id)) {
      return notFound()
    }
    if (!withReleaseContext(release, persona).allowed_actions.includes(action))
      return HttpResponse.json(
        { error: `Cannot ${action} this release`, code: 'forbidden' },
        { status: 403 },
      )
    const body = releaseActionRequestSchema.parse(await request.json())
    const next = applyAction(release, action, body.fraction)
    const history = demoState.releaseHistory[release.id] ?? []
    history.unshift({
      id: `relh-demo-${Date.now()}`,
      at: ago(0),
      actor_id: persona.userId,
      actor_email: persona.email,
      from_state: release.state,
      to_state: next.state,
      from_fraction: release.rollout_fraction,
      to_fraction: next.fraction,
      reason: body.reason,
    })
    demoState.releaseHistory[release.id] = history
    if (action === 'approve') {
      release.awaiting_approval = false
      release.approved_at = ago(0)
      release.approved_by_email = persona.email
    }
    release.state = next.state
    release.rollout_fraction = next.fraction
    release.next_fraction =
      next.state === 'rolling' ? nextFraction(release) : null
    release.next_step_at =
      release.next_fraction == null
        ? null
        : ago(-3600 * (release.policy.interval_hours ?? 24))
    release.updated_at = ago(0)
    return HttpResponse.json(withReleaseContext(release, persona))
  }
}

export const releaseHandlers = [
  demoApi.getStoreVitals(async ({ request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    return HttpResponse.json(
      demoStoreVitals(
        demoState.projects.filter((project) =>
          getDemoProjectRole(persona, project.id),
        ),
      ),
    )
  }),

  demoApi.listReleases(async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const persona = getDemoPersonaFromRequest(request)
    let releases = demoState.releases.filter((release) =>
      getDemoProjectRole(persona, release.project_id),
    )
    const projectId = url.searchParams.get('project_id')
    if (projectId) releases = releases.filter((r) => r.project_id === projectId)
    const buildId = url.searchParams.get('build_id')
    if (buildId) releases = releases.filter((r) => r.build_id === buildId)
    const states = url.searchParams.get('state')?.split(',').filter(Boolean)
    if (states?.length) {
      releases = releases.filter((release) => states.includes(release.state))
    }
    releases.sort((left, right) => right.updated_at - left.updated_at)
    const limit = Math.min(Number(url.searchParams.get('limit')) || 50, 200)
    const offset = Number(url.searchParams.get('offset')) || 0
    return HttpResponse.json({
      releases: releases
        .slice(offset, offset + limit)
        .map((release) => withReleaseContext(release, persona)),
      total: releases.length,
    })
  }),

  demoApi.getRelease(async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const release = demoState.releases.find((r) => r.id === params.release_id)
    if (!release || !getDemoProjectRole(persona, release.project_id)) {
      return notFound()
    }
    return HttpResponse.json({
      release: withReleaseContext(release, persona),
      history: demoState.releaseHistory[release.id] ?? [],
    })
  }),

  demoApi.listReleaseEvents(async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const persona = getDemoPersonaFromRequest(request)
    const projectId = url.searchParams.get('project_id')
    const limit = Math.min(Number(url.searchParams.get('limit')) || 20, 100)
    const events = demoState.releases
      .filter(
        (release) =>
          getDemoProjectRole(persona, release.project_id) &&
          (!projectId || release.project_id === projectId),
      )
      .flatMap((release) => {
        const context = withReleaseContext(release, persona)
        return (demoState.releaseHistory[release.id] ?? []).map((entry) => ({
          ...entry,
          release_id: release.id,
          project_id: release.project_id,
          project_name: context.project_name,
          build_id: release.build_id,
          store: release.store,
          track: release.track,
          version_name: release.version_name,
        }))
      })
      .sort((left, right) => right.at - left.at)
      .slice(0, limit)
    return HttpResponse.json({ events })
  }),

  demoApi.approveRelease(releaseActionHandler('approve')),
  demoApi.raiseRelease(releaseActionHandler('raise')),
  demoApi.pauseRelease(releaseActionHandler('pause')),
  demoApi.resumeRelease(releaseActionHandler('resume')),
  demoApi.haltRelease(releaseActionHandler('halt')),
  demoApi.completeRelease(releaseActionHandler('complete')),
]
