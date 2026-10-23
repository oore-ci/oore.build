import {
  BUILD_IDS,
  PIPELINE_IDS,
  PROJECT_IDS,
  RUNNER_IDS,
  USER_IDS,
  ago,
} from '../seed'
import type { Build, BuildEvent } from '@oore/client/models'

export const demoBuilds: Array<Build> = [
  // ── Running builds ────────────────────────────────────────────
  {
    id: BUILD_IDS.running1,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopAndroid,
    build_number: 142,
    status: 'running',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'mina-patel',
    trigger_ref: 'refs/heads/main',
    commit_sha: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
    branch: 'main',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(180),
        finished_at: ago(120),
        duration_ms: 60000,
      },
      {
        name: 'flutter analyze',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(120),
        finished_at: ago(60),
        duration_ms: 60000,
      },
    ],
    queued_at: ago(300),
    started_at: ago(180),
    created_at: ago(300),
    updated_at: ago(30),
  },
  {
    id: BUILD_IDS.running2,
    project_id: PROJECT_IDS.nativePayments,
    pipeline_id: PIPELINE_IDS.paymentsAll,
    build_number: 47,
    status: 'running',
    trigger_type: 'manual',
    trigger_actor: 'demo+developer@oore.build',
    commit_sha: 'f0e1d2c3b4a5f6e7d8c9b0a1f2e3d4c5b6a7f8e9',
    branch: 'main',
    changelog: 'Adds receipt sharing and improves payment retry messaging.',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macMini,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(90),
        finished_at: ago(60),
        duration_ms: 30000,
      },
    ],
    queued_at: ago(120),
    started_at: ago(90),
    created_at: ago(120),
    updated_at: ago(15),
  },

  // ── Queued build ──────────────────────────────────────────────
  {
    id: BUILD_IDS.queued1,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopIos,
    build_number: 143,
    status: 'queued',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'jordan-kim',
    trigger_ref: 'refs/heads/main',
    commit_sha: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
    branch: 'main',
    changelog: 'Adds wishlist sharing and smoother cart updates.',
    config_snapshot: {},
    queued_at: ago(60),
    created_at: ago(60),
    updated_at: ago(60),
  },

  // ── Build waiting for approval ────────────────────────────────
  {
    id: BUILD_IDS.awaitingApproval1,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopIos,
    build_number: 144,
    status: 'queued',
    trigger_type: 'webhook',
    trigger_actor: 'octocat',
    trigger_event: 'tag_push',
    trigger_ref: 'v2.4.0',
    commit_sha: 'c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3',
    approval: { state: 'pending', requested_at: ago(240) },
    config_snapshot: {},
    queued_at: ago(240),
    created_at: ago(240),
    updated_at: ago(240),
  },

  // ── Succeeded builds ──────────────────────────────────────────
  {
    id: BUILD_IDS.succeeded1,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopAndroid,
    build_number: 141,
    status: 'succeeded',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'octocat',
    trigger_ref: 'refs/heads/main',
    commit_sha: 'b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1',
    branch: 'main',
    changelog:
      'Faster checkout, clearer payment errors, and fixes for saved delivery addresses.',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(7500),
        finished_at: ago(7440),
        duration_ms: 60000,
      },
      {
        name: 'flutter analyze',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(7440),
        finished_at: ago(7380),
        duration_ms: 60000,
      },
      {
        name: 'flutter build apk --release',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(7380),
        finished_at: ago(7140),
        duration_ms: 240000,
      },
      {
        name: 'flutter test --coverage',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(7140),
        finished_at: ago(7020),
        duration_ms: 120000,
      },
    ],
    exit_code: 0,
    queued_at: ago(7560),
    started_at: ago(7500),
    finished_at: ago(7020),
    created_at: ago(7560),
    updated_at: ago(7020),
  },
  {
    id: BUILD_IDS.succeeded2,
    project_id: PROJECT_IDS.internalAdmin,
    pipeline_id: PIPELINE_IDS.adminAndroid,
    build_number: 89,
    status: 'succeeded',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'priya-shah',
    commit_sha: 'c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2',
    branch: 'develop',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macMini,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(14400),
        finished_at: ago(14340),
        duration_ms: 60000,
      },
      {
        name: 'flutter build apk --debug',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(14340),
        finished_at: ago(14160),
        duration_ms: 180000,
      },
    ],
    exit_code: 0,
    queued_at: ago(14460),
    started_at: ago(14400),
    finished_at: ago(14160),
    created_at: ago(14460),
    updated_at: ago(14160),
  },
  {
    id: BUILD_IDS.succeeded3,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopIos,
    build_number: 140,
    status: 'succeeded',
    trigger_type: 'manual',
    trigger_actor: 'demo+owner@oore.build',
    commit_sha: 'd4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3',
    branch: 'main',
    changelog:
      'Faster checkout, clearer payment errors, and fixes for saved delivery addresses.',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(28800),
        finished_at: ago(28740),
        duration_ms: 60000,
      },
      {
        name: 'flutter build ipa --release',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(28740),
        finished_at: ago(28260),
        duration_ms: 480000,
      },
    ],
    exit_code: 0,
    queued_at: ago(28860),
    started_at: ago(28800),
    finished_at: ago(28260),
    created_at: ago(28860),
    updated_at: ago(28260),
  },

  // ── Failed builds ─────────────────────────────────────────────
  {
    id: BUILD_IDS.failedPublish1,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopAndroid,
    build_number: 141,
    status: 'failed',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'mina-patel',
    trigger_ref: 'refs/heads/main',
    commit_sha: 'b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6',
    branch: 'main',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    step_results: [
      {
        name: 'checkout',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(3600),
        finished_at: ago(3590),
        duration_ms: 10000,
      },
      {
        name: 'flutter build appbundle --release',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(3590),
        finished_at: ago(3020),
        duration_ms: 570000,
      },
      {
        name: 'artifacts',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(3020),
        finished_at: ago(3000),
        duration_ms: 20000,
      },
      {
        name: 'publish-play',
        status: 'failed',
        exit_code: 1,
        started_at: ago(3000),
        finished_at: ago(2990),
        duration_ms: 10000,
      },
    ],
    exit_code: 1,
    queued_at: ago(3620),
    started_at: ago(3600),
    finished_at: ago(2990),
    created_at: ago(3620),
    updated_at: ago(2990),
  },
  {
    id: BUILD_IDS.failed1,
    project_id: PROJECT_IDS.nativePayments,
    pipeline_id: PIPELINE_IDS.paymentsAll,
    build_number: 46,
    status: 'failed',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'jordan-kim',
    commit_sha: 'e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4',
    branch: 'feature/stripe-v3',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(43200),
        finished_at: ago(43140),
        duration_ms: 60000,
      },
      {
        name: 'flutter test',
        status: 'failed',
        exit_code: 1,
        started_at: ago(43140),
        finished_at: ago(43080),
        duration_ms: 60000,
      },
    ],
    exit_code: 1,
    queued_at: ago(43260),
    started_at: ago(43200),
    finished_at: ago(43080),
    created_at: ago(43260),
    updated_at: ago(43080),
  },
  {
    id: BUILD_IDS.failed2,
    project_id: PROJECT_IDS.internalAdmin,
    pipeline_id: PIPELINE_IDS.adminAndroid,
    build_number: 88,
    status: 'failed',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'octocat',
    commit_sha: 'f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5',
    branch: 'develop',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macMini,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(57600),
        finished_at: ago(57540),
        duration_ms: 60000,
      },
      {
        name: 'flutter build apk --debug',
        status: 'failed',
        exit_code: 2,
        started_at: ago(57540),
        finished_at: ago(57300),
        duration_ms: 240000,
      },
    ],
    exit_code: 2,
    queued_at: ago(57660),
    started_at: ago(57600),
    finished_at: ago(57300),
    created_at: ago(57660),
    updated_at: ago(57300),
  },

  // ── Canceled build ────────────────────────────────────────────
  {
    id: BUILD_IDS.canceled1,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopAndroid,
    build_number: 139,
    status: 'canceled',
    trigger_type: 'manual',
    trigger_actor: 'demo+admin@oore.build',
    commit_sha: 'a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6',
    branch: 'release/1.5.0',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(72000),
        finished_at: ago(71940),
        duration_ms: 60000,
      },
    ],
    exit_code: undefined,
    queued_at: ago(72060),
    started_at: ago(72000),
    finished_at: ago(71880),
    created_at: ago(72060),
    updated_at: ago(71880),
  },

  // ── Timed-out build ───────────────────────────────────────────
  {
    id: BUILD_IDS.timedOut1,
    project_id: PROJECT_IDS.nativePayments,
    pipeline_id: PIPELINE_IDS.paymentsAll,
    build_number: 45,
    status: 'timed_out',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'priya-shah',
    commit_sha: 'b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7',
    branch: 'main',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macMini,
    step_results: [
      {
        name: 'flutter pub get',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(86400),
        finished_at: ago(86340),
        duration_ms: 60000,
      },
      {
        name: 'flutter analyze',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(86340),
        finished_at: ago(86280),
        duration_ms: 60000,
      },
      {
        name: 'flutter build apk --release',
        status: 'timed_out',
        exit_code: undefined,
        started_at: ago(86280),
        finished_at: ago(84480),
        duration_ms: 1800000,
      },
    ],
    queued_at: ago(86460),
    started_at: ago(86400),
    finished_at: ago(84480),
    created_at: ago(86460),
    updated_at: ago(84480),
  },

  // ── Older succeeded builds ────────────────────────────────────
  {
    id: BUILD_IDS.succeeded4,
    project_id: PROJECT_IDS.flutterShop,
    pipeline_id: PIPELINE_IDS.shopAndroid,
    build_number: 138,
    status: 'succeeded',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'mina-patel',
    commit_sha: 'c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8',
    branch: 'main',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    exit_code: 0,
    queued_at: ago(100800),
    started_at: ago(100740),
    finished_at: ago(100260),
    created_at: ago(100800),
    updated_at: ago(100260),
  },
  {
    id: BUILD_IDS.succeeded5,
    project_id: PROJECT_IDS.internalAdmin,
    pipeline_id: PIPELINE_IDS.adminAndroid,
    build_number: 87,
    status: 'succeeded',
    trigger_type: 'webhook',
    trigger_event: 'push',
    trigger_actor: 'jordan-kim',
    commit_sha: 'd0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9',
    branch: 'develop',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macMini,
    exit_code: 0,
    queued_at: ago(115200),
    started_at: ago(115140),
    finished_at: ago(114900),
    created_at: ago(115200),
    updated_at: ago(114900),
  },
  {
    id: BUILD_IDS.succeeded6,
    project_id: PROJECT_IDS.nativePayments,
    pipeline_id: PIPELINE_IDS.paymentsAll,
    build_number: 44,
    status: 'succeeded',
    trigger_type: 'manual',
    trigger_actor: 'demo+developer@oore.build',
    commit_sha: 'e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0',
    branch: 'release/3.2.0',
    changelog:
      'Adds saved payment methods, improves receipt sharing, and refreshes retry guidance.',
    config_snapshot: {},
    runner_id: RUNNER_IDS.macStudio,
    step_results: [
      {
        name: 'flutter analyze',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(172500),
        finished_at: ago(172440),
        duration_ms: 60000,
      },
      {
        name: 'Android release',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(172440),
        finished_at: ago(172080),
        duration_ms: 360000,
      },
      {
        name: 'iOS release',
        status: 'succeeded',
        exit_code: 0,
        started_at: ago(172080),
        finished_at: ago(171540),
        duration_ms: 540000,
      },
    ],
    exit_code: 0,
    queued_at: ago(172560),
    started_at: ago(172500),
    finished_at: ago(171540),
    created_at: ago(172560),
    updated_at: ago(171540),
  },
]

const demoBuildHistoryApps = [
  {
    projectId: PROJECT_IDS.flutterShop,
    pipelineId: PIPELINE_IDS.shopAndroid,
    branch: 'main',
    firstBuildNumber: 137,
  },
  {
    projectId: PROJECT_IDS.internalAdmin,
    pipelineId: PIPELINE_IDS.adminAndroid,
    branch: 'develop',
    firstBuildNumber: 86,
  },
  {
    projectId: PROJECT_IDS.nativePayments,
    pipelineId: PIPELINE_IDS.paymentsAll,
    branch: 'main',
    firstBuildNumber: 43,
  },
] as const

const DEMO_HISTORY_ACTORS = [
  'octocat',
  'mina-patel',
  'jordan-kim',
  'priya-shah',
]

/** A mix of pushes, pull requests, manual runs and nightly schedules. */
function demoHistoryTrigger(
  index: number,
): Pick<Build, 'trigger_actor' | 'trigger_event' | 'trigger_type'> {
  if (index % 8 === 6) {
    return { trigger_type: 'schedule', trigger_event: 'schedule' }
  }
  if (index % 8 === 3) {
    return {
      trigger_type: 'manual',
      trigger_actor: 'demo+developer@oore.build',
    }
  }
  return {
    trigger_type: 'webhook',
    trigger_event: index % 4 === 1 ? 'pull_request' : 'push',
    trigger_actor: DEMO_HISTORY_ACTORS[index % DEMO_HISTORY_ACTORS.length],
  }
}

// Typical FlutterShop Android step times, so a build's steps compare against
// the pipeline's median.
const SHOP_ANDROID_STEP_SECONDS = [
  ['flutter pub get', 40],
  ['flutter analyze', 55],
  ['flutter build apk --release', 300],
  ['flutter test --coverage', 100],
] as const

function demoShopAndroidSteps(startAge: number, index: number) {
  let age = startAge
  return SHOP_ANDROID_STEP_SECONDS.map(([name, seconds], step) => {
    const duration = seconds + (((index + 1) * (step + 3) * 7) % 21) - 10
    const result = {
      name,
      status: 'succeeded',
      exit_code: 0,
      started_at: ago(age),
      finished_at: ago(age - duration),
      duration_ms: duration * 1000,
    }
    age -= duration
    return result
  })
}

for (const [projectIndex, app] of demoBuildHistoryApps.entries()) {
  for (let index = 0; index < 24; index += 1) {
    const age = 259_200 + index * 14_400 + projectIndex * 600
    const status =
      index % 6 === 4 ? 'failed' : index % 6 === 5 ? 'canceled' : 'succeeded'
    demoBuilds.push({
      id: `build-demo-history-${projectIndex + 1}-${String(index + 1).padStart(2, '0')}`,
      project_id: app.projectId,
      pipeline_id: app.pipelineId,
      build_number: app.firstBuildNumber - index,
      status,
      ...demoHistoryTrigger(index),
      trigger_ref: `refs/heads/${app.branch}`,
      commit_sha: (projectIndex * 24 + index + 1)
        .toString(16)
        .padStart(40, '0'),
      branch: app.branch,
      changelog:
        index % 3 === 0
          ? 'Routine stability improvements and dependency updates.'
          : undefined,
      config_snapshot: {},
      runner_id: index % 2 === 0 ? RUNNER_IDS.macStudio : RUNNER_IDS.macMini,
      exit_code:
        status === 'succeeded' ? 0 : status === 'failed' ? 1 : undefined,
      step_results:
        projectIndex === 0 && status !== 'canceled'
          ? demoShopAndroidSteps(age - 60, index)
          : undefined,
      queued_at: ago(age),
      started_at: ago(age - 60),
      // Run times vary between about 5 and 11 minutes.
      finished_at: ago(age - 360 - ((index * 97 + projectIndex * 61) % 340)),
      created_at: ago(age),
      updated_at: ago(age - 360 - ((index * 97 + projectIndex * 61) % 340)),
    })
  }
}

// ── Build events for detail views ───────────────────────────────
export const demoBuildEvents = {
  [BUILD_IDS.awaitingApproval1]: [
    {
      id: 'evt-approval-001',
      build_id: BUILD_IDS.awaitingApproval1,
      to_status: 'queued',
      actor: 'octocat',
      reason: 'webhook: tag_push; waiting for approval',
      created_at: ago(240),
    },
  ],
  [BUILD_IDS.running1]: [
    {
      id: 'evt-001',
      build_id: BUILD_IDS.running1,
      to_status: 'queued',
      created_at: ago(300),
    },
    {
      id: 'evt-002',
      build_id: BUILD_IDS.running1,
      from_status: 'queued',
      to_status: 'running',
      actor: 'scheduler',
      created_at: ago(180),
    },
  ],
  [BUILD_IDS.succeeded1]: [
    {
      id: 'evt-010',
      build_id: BUILD_IDS.succeeded1,
      to_status: 'queued',
      created_at: ago(7560),
    },
    {
      id: 'evt-011',
      build_id: BUILD_IDS.succeeded1,
      from_status: 'queued',
      to_status: 'running',
      actor: 'scheduler',
      created_at: ago(7500),
    },
    {
      id: 'evt-012',
      build_id: BUILD_IDS.succeeded1,
      from_status: 'running',
      to_status: 'succeeded',
      created_at: ago(7020),
    },
  ],
  [BUILD_IDS.failed1]: [
    {
      id: 'evt-020',
      build_id: BUILD_IDS.failed1,
      to_status: 'queued',
      created_at: ago(43260),
    },
    {
      id: 'evt-021',
      build_id: BUILD_IDS.failed1,
      from_status: 'queued',
      to_status: 'running',
      actor: 'scheduler',
      created_at: ago(43200),
    },
    {
      id: 'evt-022',
      build_id: BUILD_IDS.failed1,
      from_status: 'running',
      to_status: 'failed',
      reason: 'flutter test exited with code 1',
      created_at: ago(43080),
    },
  ],
  [BUILD_IDS.canceled1]: [
    {
      id: 'evt-030',
      build_id: BUILD_IDS.canceled1,
      to_status: 'queued',
      created_at: ago(72060),
    },
    {
      id: 'evt-031',
      build_id: BUILD_IDS.canceled1,
      from_status: 'queued',
      to_status: 'running',
      actor: 'scheduler',
      created_at: ago(72000),
    },
    {
      id: 'evt-032',
      build_id: BUILD_IDS.canceled1,
      from_status: 'running',
      to_status: 'canceled',
      actor: USER_IDS.admin,
      reason: 'Canceled by user',
      created_at: ago(71880),
    },
  ],
} satisfies Record<string, Array<BuildEvent>>
