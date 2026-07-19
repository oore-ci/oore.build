import type {
  ApiTokenSummary,
  Artifact,
  ArtifactStorageSettings,
  AuditLogEntry,
  Build,
  BuildEvent,
  BuildLogChunk,
  ExternalAccessNetworkSettings,
  InstancePreferences,
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
  NotificationChannel,
  NotificationDelivery,
  Pipeline,
  Project,
  ProjectRetentionOverride,
  ProjectRole,
  RetentionCleanupSummary,
  RetentionPolicy,
  Runner,
  SetupStatus,
  TrustedProxySettingsPublic,
  User,
  UserRole,
} from '@/lib/types'
import { demoArtifacts } from './data/artifacts'
import { demoAuditLogs } from './data/audit-logs'
import { demoBuildLogs } from './data/build-logs'
import { demoBuildEvents, demoBuilds } from './data/builds'
import {
  demoInstallations,
  demoIntegrations,
  demoRepositories,
} from './data/integrations'
import {
  demoNotificationChannels,
  demoNotificationDeliveries,
} from './data/notification-channels'
import { demoPipelines } from './data/pipelines'
import {
  demoProjects,
  GITHUB_DEMO_AVATAR_URL,
  GITLAB_DEMO_AVATAR_URL,
} from './data/projects'
import { demoLastCleanup, demoRetentionPolicy } from './data/retention'
import { demoRunners } from './data/runners'
import {
  demoArtifactStorageSettings,
  demoInstancePreferences,
} from './data/settings'
import { demoUsers } from './data/users'
import {
  BUILD_IDS,
  DEMO_AUTH_EXPIRES_AT,
  DEMO_INSTANCE_ID,
  INTEGRATION_IDS,
  PIPELINE_IDS,
  PROJECT_IDS,
  RUNNER_IDS,
  USER_IDS,
  ago,
} from './seed'

export type DemoScenario =
  | 'operating'
  | 'blocked'
  | 'degraded'
  | 'empty'
  | 'setup'

export interface DemoPersona {
  userId: string
  email: string
  displayName: string
  role: UserRole
  token: string
  projectRoles: Partial<Record<string, ProjectRole>>
}

interface DemoRepositoryWorkflow {
  path: string
  valid: boolean
  errors: Array<string>
  execution: Record<string, unknown>
}

interface DemoOidcSettings {
  configured: boolean
  issuer: string
  hasClientSecret: boolean
  configuredAt: number
}

interface DemoState {
  scenario: DemoScenario
  personas: Array<DemoPersona>
  users: Array<User>
  projects: Array<Project>
  projectRoles: Partial<Record<string, Record<string, ProjectRole>>>
  pipelines: Array<Pipeline>
  repositoryWorkflows: Partial<Record<string, Array<DemoRepositoryWorkflow>>>
  builds: Array<Build>
  buildEvents: Partial<Record<string, Array<BuildEvent>>>
  buildLogs: Partial<Record<string, Array<BuildLogChunk>>>
  artifacts: Partial<Record<string, Array<Artifact>>>
  runners: Array<Runner>
  integrations: Array<Integration>
  installations: Partial<Record<string, Array<IntegrationInstallation>>>
  repositories: Partial<Record<string, Array<IntegrationRepository>>>
  notificationChannels: Array<NotificationChannel>
  notificationDeliveries: Array<NotificationDelivery>
  auditLogs: Array<AuditLogEntry>
  apiTokens: Array<ApiTokenSummary>
  artifactStorage: ArtifactStorageSettings
  preferences: InstancePreferences
  externalAccessNetwork: ExternalAccessNetworkSettings
  trustedProxy: TrustedProxySettingsPublic
  oidc: DemoOidcSettings
  retentionPolicy: RetentionPolicy
  lastCleanup: RetentionCleanupSummary | null
  projectRetentionOverrides: Partial<Record<string, ProjectRetentionOverride>>
  setupStatus: SetupStatus
  androidSigning: Partial<Record<string, Record<string, unknown>>>
  iosSigning: Partial<Record<string, Record<string, unknown>>>
  iosDevices: Partial<Record<string, Array<Record<string, unknown>>>>
}

export const EXTRA_PROJECT_IDS = {
  developerTools: 'proj-demo-devtools-004',
  workflowOnly: 'proj-demo-workflows-005',
} as const

export const EXTRA_PIPELINE_IDS = {
  developerTools: 'pipe-demo-devtools-005',
} as const

export const PAGINATED_PIPELINE_PROJECT_ID = 'proj-demo-generated-023'

export const EXTRA_BUILD_IDS = {
  scheduled: 'build-demo-scheduled-001',
  assigned: 'build-demo-assigned-001',
  expired: 'build-demo-expired-001',
  policyBlocked: 'build-demo-policy-blocked-001',
} as const

const BASE_PERSONAS: Array<DemoPersona> = [
  {
    userId: USER_IDS.owner,
    email: 'demo+owner@oore.build',
    displayName: 'Alex Chen',
    role: 'owner',
    token: 'demo-session-token-owner',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'maintainer',
      [PROJECT_IDS.internalAdmin]: 'maintainer',
      [PROJECT_IDS.nativePayments]: 'maintainer',
    },
  },
  {
    userId: USER_IDS.admin,
    email: 'demo+admin@oore.build',
    displayName: 'Jamie Park',
    role: 'admin',
    token: 'demo-session-token-admin',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'maintainer',
      [PROJECT_IDS.internalAdmin]: 'maintainer',
      [PROJECT_IDS.nativePayments]: 'maintainer',
    },
  },
  {
    userId: USER_IDS.developer,
    email: 'demo+developer@oore.build',
    displayName: 'Morgan Lee',
    role: 'developer',
    token: 'demo-session-token-developer',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'maintainer',
      [EXTRA_PROJECT_IDS.developerTools]: 'developer',
      [PROJECT_IDS.nativePayments]: 'viewer',
    },
  },
  {
    userId: USER_IDS.qaViewer,
    email: 'demo+qa@oore.build',
    displayName: 'Taylor Ruiz',
    role: 'qa_viewer',
    token: 'demo-session-token-qa',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'viewer',
      [PROJECT_IDS.nativePayments]: 'viewer',
    },
  },
]

export const DEMO_PERSONAS = BASE_PERSONAS

function clone<T>(value: T): T {
  return structuredClone(value)
}

function makeGeneratedRepositories(): Array<IntegrationRepository> {
  return Array.from({ length: 52 }, (_, index) => {
    const number = index + 4
    const suffix = String(number).padStart(3, '0')
    return {
      id: `repo-${suffix}`,
      installation_id: 'install-001',
      external_id: String(1_000 + number),
      full_name: `acme-corp/mobile-service-${suffix}`,
      default_branch: number % 3 === 0 ? 'develop' : 'main',
      is_private: number % 4 !== 0,
      allow_direct_macos_runner: number % 5 !== 0,
      created_at: ago(86400 * (40 - (number % 20))),
      updated_at: ago(3600 * number),
    }
  })
}

function makeGeneratedProjects(): Array<Project> {
  return Array.from({ length: 19 }, (_, index) => {
    const number = index + 6
    const suffix = String(number).padStart(3, '0')
    const hasSource = number !== 24
    return {
      id: `proj-demo-generated-${suffix}`,
      name: `Mobile Service ${suffix}`,
      description: `Generated demo project ${suffix} for collection pagination`,
      repository_id: hasSource ? `repo-${suffix}` : undefined,
      repository_full_name: hasSource
        ? `acme-corp/mobile-service-${suffix}`
        : undefined,
      repository_avatar_url:
        number % 2 === 0 ? GITHUB_DEMO_AVATAR_URL : GITLAB_DEMO_AVATAR_URL,
      default_branch: number % 3 === 0 ? 'develop' : 'main',
      settings: {},
      created_by: USER_IDS.owner,
      created_at: ago(86400 * number),
      updated_at: ago(3600 * number),
    }
  })
}

function makeExtraProjects(): Array<Project> {
  return [
    {
      id: EXTRA_PROJECT_IDS.developerTools,
      name: 'DeveloperTools',
      description:
        'Developer-owned tooling with project-level developer access',
      repository_id: 'repo-004',
      repository_full_name: 'acme-corp/mobile-service-004',
      repository_avatar_url: GITHUB_DEMO_AVATAR_URL,
      default_branch: 'main',
      settings: {},
      created_by: USER_IDS.developer,
      created_at: ago(86400 * 12),
      updated_at: ago(3600 * 6),
    },
    {
      id: EXTRA_PROJECT_IDS.workflowOnly,
      name: 'WorkflowDiscovery',
      description: 'Linked repository waiting for its first imported workflow',
      repository_id: 'repo-005',
      repository_full_name: 'acme-corp/mobile-service-005',
      repository_avatar_url: GITLAB_DEMO_AVATAR_URL,
      default_branch: 'main',
      settings: {},
      created_by: USER_IDS.owner,
      created_at: ago(86400 * 8),
      updated_at: ago(3600 * 2),
    },
  ]
}

function makeDeveloperPipeline(): Pipeline {
  return {
    id: EXTRA_PIPELINE_IDS.developerTools,
    project_id: EXTRA_PROJECT_IDS.developerTools,
    name: 'Tooling Checks',
    config_path: '.oore/tooling.yaml',
    config_path_explicit: true,
    execution_config: {
      platforms: ['android'],
      flutter_version: '3.24.3',
      commands: {
        pre_build: ['flutter pub get'],
        build: ['flutter test'],
        post_build: [],
      },
      artifact_patterns: ['build/reports/**'],
    },
    trigger_config: { events: ['push'], branches: ['main'] },
    concurrency: { cancel_previous: true, max_concurrent: 1 },
    enabled: true,
    created_at: ago(86400 * 10),
    updated_at: ago(3600 * 4),
  }
}

function makePaginatedPipelines(): Array<Pipeline> {
  const groups = [
    { name: 'Android smoke', count: 8, platform: 'android' },
    { name: 'iOS regression', count: 8, platform: 'ios' },
    { name: 'Release candidate', count: 9, platform: 'android' },
  ] as const

  return groups.flatMap((group, groupIndex) =>
    Array.from({ length: group.count }, (_, index) => {
      const number = String(index + 1).padStart(2, '0')
      const sequence = groupIndex * 10 + index
      return {
        id: `pipe-demo-paginated-${groupIndex + 1}-${number}`,
        project_id: PAGINATED_PIPELINE_PROJECT_ID,
        name: `${group.name} ${number}`,
        config_path: `.oore/${group.name.toLowerCase().replaceAll(' ', '-')}-${number}.yaml`,
        config_path_explicit: true,
        execution_config: {
          platforms: [group.platform],
          flutter_version: '3.24.3',
          commands: {
            pre_build: ['flutter pub get'],
            build: ['flutter test'],
            post_build: [],
          },
          artifact_patterns: ['build/reports/**'],
        },
        trigger_config: { events: ['push'], branches: ['main'] },
        concurrency: { cancel_previous: true, max_concurrent: 1 },
        enabled: true,
        created_at: ago(86400 * (30 - sequence)),
        updated_at: ago(3600 * (30 - sequence)),
      } satisfies Pipeline
    }),
  )
}

function makeExtraBuilds(): Array<Build> {
  const common = {
    project_id: EXTRA_PROJECT_IDS.developerTools,
    pipeline_id: EXTRA_PIPELINE_IDS.developerTools,
    branch: 'main',
    config_snapshot: {},
  }
  return [
    {
      ...common,
      id: EXTRA_BUILD_IDS.scheduled,
      build_number: 12,
      status: 'scheduled',
      trigger_type: 'schedule',
      queued_at: ago(900),
      created_at: ago(900),
      updated_at: ago(900),
    },
    {
      ...common,
      id: EXTRA_BUILD_IDS.assigned,
      build_number: 11,
      status: 'assigned',
      trigger_type: 'api',
      runner_id: 'runner-demo-busy-003',
      queued_at: ago(1800),
      created_at: ago(1800),
      updated_at: ago(1200),
    },
    {
      ...common,
      id: EXTRA_BUILD_IDS.expired,
      build_number: 10,
      status: 'expired',
      trigger_type: 'schedule',
      queued_at: ago(86400 * 2),
      finished_at: ago(86400),
      created_at: ago(86400 * 2),
      updated_at: ago(86400),
    },
    {
      id: EXTRA_BUILD_IDS.policyBlocked,
      project_id: PROJECT_IDS.internalAdmin,
      pipeline_id: PIPELINE_IDS.adminAndroid,
      build_number: 90,
      status: 'queued',
      trigger_type: 'api',
      branch: 'develop',
      config_snapshot: {},
      runner_policy_block_reason: 'repository_not_approved',
      queued_at: ago(600),
      created_at: ago(600),
      updated_at: ago(600),
    },
  ]
}

function makeApiTokens(): Array<ApiTokenSummary> {
  return [
    {
      id: 'token-demo-001',
      name: 'Production deploys',
      prefix: 'oore_prod',
      role: 'developer',
      created_by: USER_IDS.owner,
      created_by_email: 'demo+owner@oore.build',
      created_at: ago(86400 * 48),
      expires_at: null,
      last_used_at: ago(60 * 8),
      is_expired: false,
      is_revoked: false,
    },
    {
      id: 'token-demo-002',
      name: 'Release automation',
      prefix: 'oore_rel',
      role: 'admin',
      created_by: USER_IDS.admin,
      created_by_email: 'demo+admin@oore.build',
      created_at: ago(86400 * 21),
      expires_at: ago(-86400 * 30),
      last_used_at: ago(3600 * 3),
      is_expired: false,
      is_revoked: false,
    },
    {
      id: 'token-demo-003',
      name: 'Local CLI',
      prefix: 'oore_cli',
      role: 'developer',
      created_by: USER_IDS.developer,
      created_by_email: 'demo+developer@oore.build',
      created_at: ago(86400 * 7),
      expires_at: null,
      last_used_at: ago(3600 * 18),
      is_expired: false,
      is_revoked: false,
    },
    {
      id: 'token-demo-004',
      name: 'Old build agent',
      prefix: 'oore_old',
      role: 'developer',
      created_by: USER_IDS.developer,
      created_by_email: 'demo+developer@oore.build',
      created_at: ago(86400 * 120),
      expires_at: ago(86400 * 30),
      last_used_at: ago(86400 * 31),
      is_expired: true,
      is_revoked: false,
    },
    {
      id: 'token-demo-005',
      name: 'Retired integration',
      prefix: 'oore_ret',
      role: 'qa_viewer',
      created_by: USER_IDS.owner,
      created_by_email: 'demo+owner@oore.build',
      created_at: ago(86400 * 180),
      expires_at: null,
      last_used_at: ago(86400 * 90),
      is_expired: false,
      is_revoked: true,
    },
  ]
}

function makeProjectRoles(
  personas: Array<DemoPersona>,
): Record<string, Record<string, ProjectRole>> {
  const roles: Record<string, Record<string, ProjectRole>> = {}
  for (const persona of personas) {
    for (const [projectId, role] of Object.entries(persona.projectRoles)) {
      if (!role) continue
      roles[projectId] ??= {}
      roles[projectId][persona.userId] = role
    }
  }
  return roles
}

export function createDemoState(
  scenario: DemoScenario = 'operating',
): DemoState {
  const personas = clone(BASE_PERSONAS)
  const projects = [
    ...clone(demoProjects).map((project) => ({
      ...project,
      repository_id: project.repository_id?.split(':').at(-1),
    })),
    ...makeExtraProjects(),
    ...makeGeneratedProjects(),
  ]
  const pipelines = [
    ...clone(demoPipelines),
    makeDeveloperPipeline(),
    ...makePaginatedPipelines(),
  ]
  const builds = [...clone(demoBuilds), ...makeExtraBuilds()]
  const repositories = clone(demoRepositories)
  repositories[INTEGRATION_IDS.github] = [
    ...(repositories[INTEGRATION_IDS.github] ?? []),
    ...makeGeneratedRepositories(),
  ]
  const users = [
    ...clone(demoUsers),
    {
      id: 'usr-demo-disabled-006',
      email: 'demo+disabled@oore.build',
      display_name: 'Riley Disabled',
      role: 'developer' as const,
      status: 'disabled' as const,
      created_at: ago(86400 * 45),
      updated_at: ago(86400 * 4),
    },
  ]
  const runners = [
    ...clone(demoRunners),
    {
      id: 'runner-demo-busy-003',
      name: 'mac-studio-busy',
      status: 'busy',
      capabilities: { os: 'macOS', arch: 'arm64', xcode: '15.4' },
      last_heartbeat_at: ago(8),
      registered_by: USER_IDS.admin,
      created_at: ago(86400 * 30),
      updated_at: ago(8),
    },
    {
      id: 'runner-demo-draining-004',
      name: 'mac-mini-draining',
      status: 'draining',
      capabilities: { os: 'macOS', arch: 'arm64', xcode: '15.4' },
      last_heartbeat_at: ago(20),
      registered_by: USER_IDS.owner,
      created_at: ago(86400 * 25),
      updated_at: ago(20),
    },
  ] satisfies Array<Runner>
  const notificationChannels = [
    ...clone(demoNotificationChannels),
    {
      id: 'notif-demo-disabled-004',
      name: 'Paused release webhook',
      channel_type: 'webhook' as const,
      enabled: false,
      events: ['succeeded'],
      has_url: true,
      has_secret: false,
      has_smtp_config: false,
      created_by: USER_IDS.admin,
      created_at: ago(86400 * 20),
      updated_at: ago(86400 * 5),
    },
  ]
  const notificationDeliveries = [
    ...clone(demoNotificationDeliveries),
    {
      id: 'delivery-demo-pending-006',
      channel_id: notificationChannels[0].id,
      build_id: EXTRA_BUILD_IDS.scheduled,
      event_type: 'scheduled',
      status: 'pending' as const,
      attempt_count: 0,
      created_at: ago(120),
    },
  ]
  const artifacts = clone(demoArtifacts)
  artifacts[BUILD_IDS.succeeded1] = [
    ...(artifacts[BUILD_IDS.succeeded1] ?? []),
    {
      id: 'art-demo-app-008',
      build_id: BUILD_IDS.succeeded1,
      name: 'FlutterShop.app',
      artifact_type: 'app',
      file_path: 'build/macos/Build/Products/Release/FlutterShop.app',
      file_size: 72_351_744,
      metadata: { platform: 'macos' },
      created_at: ago(7000),
    },
    {
      id: 'art-demo-generic-009',
      build_id: BUILD_IDS.succeeded1,
      name: 'coverage-report.zip',
      artifact_type: 'generic',
      file_path: 'build/reports/coverage-report.zip',
      file_size: 1_572_864,
      checksum: 'sha256:demo-coverage',
      metadata: { report: 'coverage' },
      created_at: ago(6990),
    },
  ]

  const state: DemoState = {
    scenario,
    personas,
    users,
    projects,
    projectRoles: makeProjectRoles(personas),
    pipelines,
    repositoryWorkflows: {
      [EXTRA_PROJECT_IDS.workflowOnly]: [
        {
          path: '.oore/release.yaml',
          valid: true,
          errors: [],
          execution: {
            platforms: ['android', 'ios'],
            flutter_version: '3.24.3',
            commands: { pre_build: ['flutter pub get'], build: [] },
            artifact_patterns: [
              'build/app/outputs/**/*.apk',
              'build/ios/**/*.ipa',
            ],
          },
        },
        {
          path: '.oore/invalid.yaml',
          valid: false,
          errors: ['execution.platforms must contain at least one platform'],
          execution: {},
        },
      ],
    },
    builds,
    buildEvents: clone(demoBuildEvents),
    buildLogs: clone(demoBuildLogs),
    artifacts,
    runners,
    integrations: clone(demoIntegrations),
    installations: clone(demoInstallations),
    repositories,
    notificationChannels,
    notificationDeliveries,
    auditLogs: clone(demoAuditLogs),
    apiTokens: makeApiTokens(),
    artifactStorage: clone(demoArtifactStorageSettings),
    preferences: clone(demoInstancePreferences),
    externalAccessNetwork: {
      allowed_origins: [],
      source: 'default',
      updated_at: ago(86400 * 30),
    },
    trustedProxy: {
      user_email_header: 'x-oore-user-email',
      trusted_proxy_cidrs: [],
      has_shared_secret: false,
      has_warpgate_ticket: false,
      updated_at: ago(86400 * 30),
    },
    oidc: {
      configured: true,
      issuer: 'https://accounts.google.com',
      hasClientSecret: false,
      configuredAt: ago(86400 * 30),
    },
    retentionPolicy: clone(demoRetentionPolicy),
    lastCleanup: clone(demoLastCleanup),
    projectRetentionOverrides: {},
    setupStatus: {
      instance_id: DEMO_INSTANCE_ID,
      state: 'ready',
      runtime_mode: 'remote',
      remote_auth_mode: 'oidc',
      setup_mode: false,
      is_configured: true,
    },
    androidSigning: {},
    iosSigning: {},
    iosDevices: {},
  }

  if (scenario === 'blocked') {
    state.preferences.direct_macos_runner_enabled = false
    const blocked = state.builds.find(
      (build) => build.id === EXTRA_BUILD_IDS.policyBlocked,
    )
    if (blocked) blocked.runner_policy_block_reason = 'instance_disabled'
  }

  if (scenario === 'degraded') {
    const github = state.integrations.find(
      (integration) => integration.id === INTEGRATION_IDS.github,
    )
    if (github) github.status = 'error'
    for (const runner of state.runners) {
      runner.status =
        runner.id === RUNNER_IDS.macStudio ? 'draining' : 'offline'
    }
    const blocked = state.builds.find(
      (build) => build.id === EXTRA_BUILD_IDS.policyBlocked,
    )
    if (blocked) blocked.runner_policy_block_reason = 'repository_unavailable'
  }

  if (scenario === 'empty' || scenario === 'setup') {
    state.projects = []
    state.projectRoles = {}
    state.pipelines = []
    state.repositoryWorkflows = {}
    state.builds = []
    state.buildEvents = {}
    state.buildLogs = {}
    state.artifacts = {}
    state.runners = []
    state.integrations = []
    state.installations = {}
    state.repositories = {}
    state.notificationChannels = []
    state.notificationDeliveries = []
    state.auditLogs = []
  }

  if (scenario === 'setup') {
    state.setupStatus = {
      instance_id: DEMO_INSTANCE_ID,
      state: 'bootstrap_pending',
      runtime_mode: 'local',
      remote_auth_mode: 'oidc',
      setup_mode: true,
      is_configured: false,
    }
    state.preferences.runtime_mode = 'local'
  }

  return state
}

export const demoState = createDemoState()

export function resetDemoState(
  scenario: DemoScenario = 'operating',
): DemoState {
  Object.assign(demoState, createDemoState(scenario))
  return demoState
}

export function readDemoScenario(search: string): DemoScenario {
  const candidate = new URLSearchParams(search).get('demoScenario')
  return candidate === 'blocked' ||
    candidate === 'degraded' ||
    candidate === 'empty' ||
    candidate === 'setup'
    ? candidate
    : 'operating'
}

export function demoSessionExpiresAt(): number {
  return DEMO_AUTH_EXPIRES_AT
}
