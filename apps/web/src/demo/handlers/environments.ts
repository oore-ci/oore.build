import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import type {
  CreateProjectEnvironmentRequest,
  CreatePromotionRequest,
  ReleasePolicy,
  UpdateProjectEnvironmentRequest,
} from '@oore/client/models'
import { ago } from '../seed'
import type { DemoEnvironment, DemoPromotion } from '../data/environments'
import type { DemoRelease } from '../data/releases'
import type { DemoPersona } from '../personas'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { demoState } from '../state'
import { demoApi } from './api'
import { withReleaseContext } from './releases'

const policySchema = z.object({
  steps: z.array(z.number()).optional(),
  interval_hours: z.number().optional(),
  crash_rate_threshold: z.number().nullish(),
  approval: z.enum(['none', 'before_upload', 'before_production']).optional(),
})

const createSchema = z.toZod<CreateProjectEnvironmentRequest>()(
  z.object({
    key: z.string(),
    name: z.string(),
    store: z.enum(['google_play', 'app_store']),
    track: z.string(),
    approval_required: z.boolean().optional(),
    rollout_policy: policySchema.nullish(),
    submit_for_review: z.boolean().optional(),
  }),
)

const updateSchema = z.toZod<UpdateProjectEnvironmentRequest>()(
  z.object({
    name: z.string().nullish(),
    track: z.string().nullish(),
    approval_required: z.boolean().nullish(),
    rollout_policy: policySchema.nullish(),
    submit_for_review: z.boolean().nullish(),
    position: z.number().nullish(),
  }),
)

const promoteSchema = z.toZod<CreatePromotionRequest>()(
  z.object({
    build_id: z.string(),
    environment_id: z.string(),
    reason: z.string().nullish(),
  }),
)

const decisionSchema = z.object({ reason: z.string().nullish() })

function error(status: number, code: string, message: string) {
  return HttpResponse.json({ error: message, code }, { status })
}

function emailOf(userId: string | undefined) {
  return userId
    ? demoState.users.find((user) => user.id === userId)?.email
    : undefined
}

function environmentName(environmentId: string | null | undefined) {
  return demoState.environments.find((item) => item.id === environmentId)?.name
}

function withPromotionContext(promotion: DemoPromotion, persona: DemoPersona) {
  const role = getDemoProjectRole(persona, promotion.project_id)
  const source = demoState.releases.find(
    (release) => release.id === promotion.source_release_id,
  )
  return {
    ...promotion,
    environment_name: environmentName(promotion.environment_id) ?? 'Deleted',
    source_environment_name: environmentName(source?.environment_id),
    requested_by_email: emailOf(promotion.requested_by),
    decided_by_email: emailOf(promotion.decided_by),
    can_decide: promotion.state === 'pending' && role === 'maintainer',
  }
}

function withEnvironmentContext(
  environment: DemoEnvironment,
  persona: DemoPersona,
) {
  const latest = demoState.releases
    .filter((release) => release.environment_id === environment.id)
    .sort((left, right) => right.created_at - left.created_at)[0]
  return {
    ...environment,
    latest_release: latest ? withReleaseContext(latest, persona) : undefined,
  }
}

const DEFAULT_POLICY: ReleasePolicy = {
  steps: [0.01, 0.05, 0.2, 0.5, 1],
  interval_hours: 24,
  approval: 'none',
}

function promote(
  promotion: DemoPromotion,
  environment: DemoEnvironment,
  persona: DemoPersona,
) {
  const source = demoState.releases.find(
    (release) => release.id === promotion.source_release_id,
  )
  if (!source) return
  const steps = environment.rollout_policy.steps ?? [1]
  const appStore = environment.store === 'app_store'
  // An App Store version waits for App Review; TestFlight testers have it at once.
  const inReview = appStore && environment.track === 'app_store'
  const first =
    appStore || environment.track === 'internal' ? 1 : (steps.at(0) ?? 1)
  const release: DemoRelease = {
    ...source,
    id: `rel-demo-promoted-${Date.now()}`,
    track: environment.track,
    state: inReview ? 'in_review' : first >= 1 ? 'complete' : 'rolling',
    rollout_fraction: inReview ? 0 : first,
    next_fraction:
      first >= 1 ? null : (steps.find((step) => step > first) ?? null),
    next_step_at:
      first >= 1
        ? null
        : ago(-3600 * (environment.rollout_policy.interval_hours ?? 24)),
    policy: environment.rollout_policy,
    crash_rate_threshold: environment.rollout_policy.crash_rate_threshold,
    environment_id: environment.id,
    environment_name: environment.name,
    promoted_from_release_id: source.id,
    awaiting_approval: false,
    created_at: ago(0),
    updated_at: ago(0),
  }
  demoState.releases.unshift(release)
  demoState.releaseHistory[release.id] = [
    {
      id: `relh-demo-${Date.now()}`,
      at: ago(0),
      actor_id: persona.userId,
      actor_email: persona.email,
      to_state: release.state,
      to_fraction: release.rollout_fraction,
      reason: `Promoted from ${source.environment_name ?? `the ${source.track} track`} without a new build${
        inReview
          ? environment.submit_for_review
            ? `. Submitted version ${source.version_name ?? ''} for App Store review`
            : `. Build attached to version ${source.version_name ?? ''}; submit it for review in App Store Connect`
          : appStore
            ? `. Available in TestFlight to ${environment.track}`
            : ''
      }`,
    },
  ]
  promotion.state = 'completed'
  promotion.release_id = release.id
  promotion.updated_at = ago(0)
}

export const environmentHandlers = [
  demoApi.listProjectEnvironments(async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const projectId = String(params.project_id)
    if (!getDemoProjectRole(persona, projectId)) {
      return error(404, 'not_found', 'Project not found')
    }
    const environments = demoState.environments
      .filter((environment) => environment.project_id === projectId)
      .sort((left, right) => left.position - right.position)
      .map((environment) => withEnvironmentContext(environment, persona))
    const promotions = demoState.promotions
      .filter((promotion) => promotion.project_id === projectId)
      .sort((left, right) => right.created_at - left.created_at)
      .slice(0, 25)
      .map((promotion) => withPromotionContext(promotion, persona))
    return HttpResponse.json({ environments, promotions })
  }),

  demoApi.createProjectEnvironment(async ({ params, request }) => {
    await delay(250)
    const persona = getDemoPersonaFromRequest(request)
    const projectId = String(params.project_id)
    const role = getDemoProjectRole(persona, projectId)
    if (!role) return error(404, 'not_found', 'Project not found')
    if (role !== 'maintainer') {
      return error(403, 'permission_denied', 'Maintainer role required')
    }
    const body = createSchema.parse(await request.json())
    const siblings = demoState.environments.filter(
      (environment) => environment.project_id === projectId,
    )
    if (
      siblings.some(
        (environment) =>
          environment.key === body.key ||
          (environment.store === body.store &&
            environment.track === body.track),
      )
    ) {
      return error(
        409,
        'environment_exists',
        'Another environment already uses this key, or this store and track',
      )
    }
    const environment: DemoEnvironment = {
      id: `env-demo-${Date.now()}`,
      project_id: projectId,
      key: body.key,
      name: body.name,
      position: siblings.length,
      store: body.store,
      track: body.track,
      approval_required: body.approval_required ?? false,
      rollout_policy: body.rollout_policy ?? DEFAULT_POLICY,
      submit_for_review: body.submit_for_review ?? false,
      promotable: true,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.environments.push(environment)
    return HttpResponse.json(withEnvironmentContext(environment, persona))
  }),

  demoApi.updateProjectEnvironment(async ({ params, request }) => {
    await delay(250)
    const persona = getDemoPersonaFromRequest(request)
    const environment = demoState.environments.find(
      (item) =>
        item.id === params.environment_id &&
        item.project_id === params.project_id,
    )
    if (!environment || !getDemoProjectRole(persona, environment.project_id)) {
      return error(404, 'not_found', 'Environment not found')
    }
    if (getDemoProjectRole(persona, environment.project_id) !== 'maintainer') {
      return error(403, 'permission_denied', 'Maintainer role required')
    }
    const body = updateSchema.parse(await request.json())
    if (
      body.track &&
      body.track !== environment.track &&
      demoState.releases.some(
        (release) => release.environment_id === environment.id,
      )
    ) {
      return error(
        400,
        'invalid_input',
        `${environment.name} already has releases on the ${environment.track} track, so its track cannot change. Add a new environment for the ${body.track} track instead.`,
      )
    }
    environment.name = body.name ?? environment.name
    environment.track = body.track ?? environment.track
    environment.submit_for_review =
      body.submit_for_review ?? environment.submit_for_review
    environment.approval_required =
      body.approval_required ?? environment.approval_required
    environment.rollout_policy =
      body.rollout_policy ?? environment.rollout_policy
    environment.position = body.position ?? environment.position
    environment.updated_at = ago(0)
    return HttpResponse.json(withEnvironmentContext(environment, persona))
  }),

  demoApi.deleteProjectEnvironment(async ({ params, request }) => {
    await delay(250)
    const persona = getDemoPersonaFromRequest(request)
    const environment = demoState.environments.find(
      (item) =>
        item.id === params.environment_id &&
        item.project_id === params.project_id,
    )
    if (!environment || !getDemoProjectRole(persona, environment.project_id)) {
      return error(404, 'not_found', 'Environment not found')
    }
    if (getDemoProjectRole(persona, environment.project_id) !== 'maintainer') {
      return error(403, 'permission_denied', 'Maintainer role required')
    }
    demoState.environments = demoState.environments.filter(
      (item) => item.id !== environment.id,
    )
    demoState.promotions = demoState.promotions.filter(
      (promotion) => promotion.environment_id !== environment.id,
    )
    for (const release of demoState.releases) {
      if (release.environment_id === environment.id) {
        release.environment_id = null
        release.environment_name = null
      }
    }
    return HttpResponse.json({ ok: true })
  }),

  demoApi.promoteBuild(async ({ params, request }) => {
    await delay(400)
    const persona = getDemoPersonaFromRequest(request)
    const projectId = String(params.project_id)
    const role = getDemoProjectRole(persona, projectId)
    if (!role) return error(404, 'not_found', 'Project not found')
    const body = promoteSchema.parse(await request.json())
    const environment = demoState.environments.find(
      (item) =>
        item.id === body.environment_id && item.project_id === projectId,
    )
    if (!environment) return error(404, 'not_found', 'Environment not found')
    const allowed = environment.approval_required
      ? role !== 'viewer'
      : role === 'maintainer'
    if (!allowed) {
      return error(403, 'permission_denied', 'Maintainer role required')
    }
    if (
      demoState.releases.some(
        (release) =>
          release.build_id === body.build_id &&
          release.environment_id === environment.id &&
          release.state !== 'failed',
      )
    ) {
      return error(
        409,
        'already_in_environment',
        `This build already has a release in ${environment.name}`,
      )
    }
    if (
      demoState.promotions.some(
        (promotion) =>
          promotion.build_id === body.build_id &&
          promotion.environment_id === environment.id &&
          promotion.state === 'pending',
      )
    ) {
      return error(409, 'promotion_pending', 'Already waiting for approval')
    }
    const source = demoState.releases
      .filter(
        (release) =>
          release.build_id === body.build_id &&
          release.store === environment.store &&
          !(release.store === 'app_store' && release.track === 'app_store') &&
          ['rolling', 'paused', 'complete'].includes(release.state) &&
          release.environment_id !== environment.id,
      )
      .sort((left, right) => right.created_at - left.created_at)[0]
    if (!source) {
      return error(
        409,
        'no_source_release',
        environment.store === 'app_store'
          ? 'This build has no TestFlight release to promote.'
          : 'This build has no live Google Play release to promote.',
      )
    }
    const promotion: DemoPromotion = {
      id: `promo-demo-${Date.now()}`,
      project_id: projectId,
      build_id: body.build_id,
      build_number: source.build_number,
      version_name: source.version_name,
      version_code: source.version_code,
      environment_id: environment.id,
      source_release_id: source.id,
      source_track: source.track,
      state: environment.approval_required ? 'pending' : 'promoting',
      reason: body.reason,
      requested_by: persona.userId,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.promotions.unshift(promotion)
    if (!environment.approval_required) promote(promotion, environment, persona)
    return HttpResponse.json(withPromotionContext(promotion, persona))
  }),

  ...(['approvePromotion', 'rejectPromotion'] as const).map((operation) =>
    demoApi[operation](async ({ params, request }) => {
      await delay(400)
      const persona = getDemoPersonaFromRequest(request)
      const promotion = demoState.promotions.find(
        (item) =>
          item.id === params.promotion_id &&
          item.project_id === params.project_id,
      )
      if (!promotion || !getDemoProjectRole(persona, promotion.project_id)) {
        return error(404, 'not_found', 'Promotion not found')
      }
      if (getDemoProjectRole(persona, promotion.project_id) !== 'maintainer') {
        return error(403, 'permission_denied', 'Maintainer role required')
      }
      if (promotion.state !== 'pending') {
        return error(409, 'promotion_not_pending', 'Not waiting for approval')
      }
      const body = decisionSchema.parse(await request.json().catch(() => ({})))
      promotion.decided_by = persona.userId
      promotion.decided_at = ago(0)
      promotion.decision_reason = body.reason
      const environment = demoState.environments.find(
        (item) => item.id === promotion.environment_id,
      )
      if (operation === 'rejectPromotion' || !environment) {
        promotion.state = 'rejected'
        promotion.updated_at = ago(0)
      } else {
        promote(promotion, environment, persona)
      }
      return HttpResponse.json(withPromotionContext(promotion, persona))
    }),
  ),
]
