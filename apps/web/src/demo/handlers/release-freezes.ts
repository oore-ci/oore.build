import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import type {
  BuildFailureHint,
  BuildTrendsResponse,
  ReleaseFreeze,
} from '@oore/client/models'

import { demoApi } from './api'
import { withReleaseContext } from './releases'
import { requireDemoProjectPermission } from '../authorization'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { BUILD_IDS, PIPELINE_IDS, PROJECT_IDS, ago } from '../seed'
import { demoState } from '../state'

const DAY = 86_400

const freezeSchema = z.object({
  project_id: z.string().nullish(),
  reason: z.string(),
})
const recordSchema = z.object({
  build_id: z.string(),
  store: z.enum(['google_play', 'app_store']),
  track: z.string(),
  version_name: z.string().nullish(),
  version_code: z.number().nullish(),
})

const freezes = new Map<string, ReleaseFreeze>()

/** The freeze that holds an app's releases, for the release handlers. */
export function demoActiveFreeze(projectId: string): ReleaseFreeze | null {
  const all = [...freezes.values()]
  return (
    all.find((freeze) => freeze.project_id === projectId) ??
    all.find((freeze) => !freeze.project_id) ??
    null
  )
}

export function demoFrozenResponse(freeze: ReleaseFreeze) {
  return HttpResponse.json(
    {
      error: `${freeze.project_name ? `${freeze.project_name} is frozen` : 'Releases are frozen'}: ${freeze.reason}. Lift the freeze to continue`,
      code: 'release_frozen',
    },
    { status: 409 },
  )
}

const FAILURE_HINTS = new Map<string, BuildFailureHint>([
  [
    BUILD_IDS.failed2,
    {
      step: null,
      line: "Execution failed for task ':app:compileDebugKotlin'.",
      sequence: 6,
      category: 'gradle',
      cause: 'Kotlin code in the Android app failed to compile.',
      fix: 'Run `flutter build apk` locally to see the compiler error; a plugin update often needs a newer Kotlin Gradle plugin.',
    },
  ],
])

function demoTrends(days: number): BuildTrendsResponse {
  return {
    window_days: days,
    flaky_tests: [
      {
        project_id: PROJECT_IDS.nativePayments,
        project_name: 'Payments',
        pipeline_id: PIPELINE_IDS.paymentsAll,
        pipeline_name: 'All platforms',
        suite: 'test/payment_flow_test.dart',
        name: 'card payment times out on slow network',
        occurrences: 3,
        failed_build_id: BUILD_IDS.failed1,
        failed_build_number: 46,
        passed_build_id: BUILD_IDS.succeeded3,
        passed_build_number: 47,
        last_seen_at: ago(DAY / 2),
      },
      {
        project_id: PROJECT_IDS.flutterShop,
        project_name: 'Shop',
        pipeline_id: PIPELINE_IDS.shopAndroid,
        pipeline_name: 'Android release',
        suite: 'test/cart_test.dart',
        name: 'restores the cart after sign-in',
        occurrences: 1,
        failed_build_id: BUILD_IDS.timedOut1,
        failed_build_number: 139,
        passed_build_id: BUILD_IDS.succeeded1,
        passed_build_number: 140,
        last_seen_at: ago(3 * DAY),
      },
    ],
    slower_steps: [
      {
        project_id: PROJECT_IDS.flutterShop,
        project_name: 'Shop',
        pipeline_id: PIPELINE_IDS.shopIos,
        pipeline_name: 'iOS release',
        step: 'flutter build ipa',
        before_seconds: 412,
        after_seconds: 598,
        builds: 24,
      },
    ],
  }
}

export const releaseFreezeHandlers = [
  demoApi.listReleaseFreezes(async ({ request }) => {
    await delay(100)
    const persona = getDemoPersonaFromRequest(request)
    return HttpResponse.json({
      freezes: [...freezes.values()].filter(
        (freeze) =>
          !freeze.project_id || getDemoProjectRole(persona, freeze.project_id),
      ),
    })
  }),

  demoApi.setReleaseFreeze(async ({ request }) => {
    const body = freezeSchema.parse(await request.json())
    if (body.project_id) {
      const forbidden = requireDemoProjectPermission(
        request,
        body.project_id,
        'projects:write',
      )
      if (forbidden) return forbidden
    }
    const scope = body.project_id ?? '*'
    const freeze: ReleaseFreeze = {
      id: `freeze-demo-${scope}`,
      project_id: body.project_id ?? null,
      project_name: body.project_id
        ? (demoState.projects.find((p) => p.id === body.project_id)?.name ??
          null)
        : null,
      reason: body.reason.trim(),
      created_by_email: getDemoPersonaFromRequest(request).email,
      created_at: ago(0),
    }
    freezes.set(scope, freeze)
    return HttpResponse.json(freeze)
  }),

  demoApi.deleteReleaseFreeze(({ params }) => {
    for (const [scope, freeze] of freezes) {
      if (freeze.id === params.freeze_id) freezes.delete(scope)
    }
    return new HttpResponse(null, { status: 204 })
  }),

  demoApi.createRelease(async ({ request }) => {
    await delay(200)
    const body = recordSchema.parse(await request.json())
    const build = demoState.builds.find((b) => b.id === body.build_id)
    if (!build) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const forbidden = requireDemoProjectPermission(
      request,
      build.project_id,
      'projects:write',
    )
    if (forbidden) return forbidden
    const release = {
      id: `rel-demo-${crypto.randomUUID().slice(0, 8)}`,
      project_id: build.project_id,
      build_id: build.id,
      build_number: build.build_number,
      artifact_id: null,
      platform: body.store === 'app_store' ? 'ios' : 'android',
      store: body.store,
      track: body.track,
      package_name: null,
      version_name: body.version_name ?? null,
      version_code: body.version_code ?? null,
      state: 'complete' as const,
      state_reason: null,
      origin: 'oore' as const,
      auto_rollout: false,
      rollout_fraction: 1,
      next_fraction: null,
      next_step_at: null,
      health: 'unknown' as const,
      crash_rate: null,
      crash_rate_threshold: null,
      policy: { steps: [1], interval_hours: 24, approval: 'none' as const },
      awaiting_approval: false,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.releases.push(release)
    demoState.releaseHistory[release.id] = [
      {
        id: `hist-${release.id}`,
        actor_email: getDemoPersonaFromRequest(request).email,
        from_state: null,
        to_state: 'complete',
        from_fraction: null,
        to_fraction: 1,
        reason: 'Uploaded by hand in the store console',
        at: ago(0),
      },
    ]
    return HttpResponse.json(
      withReleaseContext(release, getDemoPersonaFromRequest(request)),
    )
  }),

  demoApi.getBuildFailureHint(async ({ params }) => {
    await delay(150)
    return HttpResponse.json({
      hint: FAILURE_HINTS.get(String(params.build_id)) ?? null,
    })
  }),

  demoApi.getBuildTrends(async ({ request }) => {
    await delay(200)
    const days = Number(new URL(request.url).searchParams.get('days') ?? 30)
    return HttpResponse.json(demoTrends(days))
  }),
]
