import {
  BUILD_IDS,
  INTEGRATION_IDS,
  NOTIFICATION_CHANNEL_IDS,
  PIPELINE_IDS,
  PROJECT_IDS,
  RUNNER_IDS,
  USER_IDS,
  ago,
} from '../seed'
import type { AuditLogEntry } from '@oore/client/models'

const MINUTE = 60
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

const EMAILS = new Map<string, string>([
  [USER_IDS.owner, 'demo+owner@oore.build'],
  [USER_IDS.admin, 'demo+admin@oore.build'],
  [USER_IDS.developer, 'demo+developer@oore.build'],
  [USER_IDS.qaViewer, 'demo+qa@oore.build'],
])

type DetailValue =
  | string
  | number
  | boolean
  | null
  | undefined
  | Array<string>
  | Readonly<Record<string, string | number>>

type Row = [
  secondsAgo: number,
  actor: string | null,
  action: string,
  resourceType: string,
  resourceId: string | null,
  details?: Readonly<Record<string, DetailValue>>,
  /** Details the daemon writes as plain words instead of JSON. */
  note?: string,
  /** The API token the actor acted through, by demo token id. */
  token?: keyof typeof TOKENS,
]

/** Demo API tokens that show up in the log, matching `makeApiTokens`. */
const TOKENS = {
  'token-demo-002': 'Release automation',
  'token-demo-003': 'Local CLI',
} as const

const { owner, admin, developer, qaViewer } = USER_IDS

const signIn = (secondsAgo: number, user: string, ip = '10.4.2.18'): Row => [
  secondsAgo,
  user,
  'trusted_proxy_login_succeeded',
  'auth',
  user,
  { email: EMAILS.get(user), peer_ip: ip },
]

const cleanup = (secondsAgo: number, deleted = 0, tokens = 0): Row => [
  secondsAgo,
  null,
  'artifact_expiry_cleanup_completed',
  'artifact',
  null,
  {
    artifacts_deleted: deleted,
    bytes_reclaimed: deleted * 48_500_000,
    tokens_deleted: tokens,
    ran_at: ago(secondsAgo),
  },
]

const started = (
  secondsAgo: number,
  user: string,
  buildId: string,
  projectId: string,
  pipelineId: string,
  buildNumber: number,
): Row => [
  secondsAgo,
  user,
  'build_created',
  'build',
  buildId,
  {
    project_id: projectId,
    pipeline_id: pipelineId,
    build_number: buildNumber,
    trigger_type: 'manual',
    created_by: EMAILS.get(user),
  },
]

const withToken = (row: Row, token: keyof typeof TOKENS): Row => {
  const [secondsAgo, actor, action, resourceType, resourceId, details, note] =
    row
  return [
    secondsAgo,
    actor,
    action,
    resourceType,
    resourceId,
    details,
    note,
    token,
  ]
}

// The same actions and details the daemon writes, newest first.
const ROWS: Array<Row> = [
  [
    4 * MINUTE,
    owner,
    'external_access_preflight_checked',
    'instance_settings',
    'external_access',
    { failed_checks: [], ready: true },
  ],
  signIn(9 * MINUTE, owner),
  withToken(
    started(
      22 * MINUTE,
      developer,
      BUILD_IDS.running2,
      PROJECT_IDS.nativePayments,
      PIPELINE_IDS.paymentsAll,
      47,
    ),
    'token-demo-003',
  ),
  [
    48 * MINUTE,
    admin,
    'artifact_install_link_generated',
    'artifact_download_token',
    'tok-demo-install-482',
    {
      artifact_id: 'art-002',
      platform: 'android',
      token_prefix: 'bbd83621',
      expires_at: ago(-7 * DAY),
    },
  ],
  signIn(52 * MINUTE, admin),
  cleanup(70 * MINUTE, 0, 1),
  [
    95 * MINUTE,
    owner,
    'role_changed',
    'user',
    qaViewer,
    {
      from_role: 'qa_viewer',
      to_role: 'developer',
      changed_by: EMAILS.get(owner),
    },
  ],
  [
    2 * HOUR,
    admin,
    'build_sent_to_testers',
    'build',
    BUILD_IDS.succeeded1,
    { group_id: 'grp-demo-qa', recipients: 12 },
    undefined,
    'token-demo-002',
  ],
  [
    2.5 * HOUR,
    null,
    'trusted_proxy_login_blocked_untrusted_peer',
    'auth',
    null,
    { peer_ip: '203.0.113.44' },
  ],
  [
    3 * HOUR,
    owner,
    'api_token_created',
    'api_token',
    'tok-demo-ci-deploy',
    undefined,
    'name=ci-deploy, role=developer, prefix=oore_7f3a91c2, abilities=read build',
  ],
  cleanup(5 * HOUR),
  [
    6 * HOUR,
    developer,
    'build_canceled',
    'build',
    BUILD_IDS.canceled1,
    { canceled_by: EMAILS.get(developer), from_status: 'running' },
  ],
  [
    7 * HOUR,
    owner,
    'release_freeze_set',
    'project',
    PROJECT_IDS.nativePayments,
    { reason: 'Payment gateway migration' },
  ],
  cleanup(9 * HOUR),
  [
    10 * HOUR,
    null,
    'store_upload_completed',
    'build',
    BUILD_IDS.succeeded7,
    {
      build_id: BUILD_IDS.succeeded7,
      pipeline_id: PIPELINE_IDS.shopAndroid,
      runner_id: RUNNER_IDS.macStudio,
      release_id: 'rel-demo-play-internal',
      result: { track: 'internal', version_code: 145 },
    },
  ],
  [
    11 * HOUR,
    admin,
    'pipeline_app_store_publish_updated',
    'pipeline',
    PIPELINE_IDS.shopIos,
    {
      pipeline_id: PIPELINE_IDS.shopIos,
      enabled: true,
      destination: 'testflight',
      submit_for_review: false,
      phased_release: true,
      testflight_groups: ['Internal', 'QA'],
    },
  ],
  signIn(12 * HOUR, developer, '10.4.2.31'),
  cleanup(13 * HOUR, 2),
  [
    DAY + 1 * HOUR,
    owner,
    'user_invited',
    'user',
    USER_IDS.invited,
    {
      email: 'sam@example.com',
      role: 'developer',
      invited_by: EMAILS.get(owner),
    },
  ],
  [
    DAY + 2 * HOUR,
    owner,
    'project_member_added',
    'project',
    PROJECT_IDS.nativePayments,
    {
      added_user_id: developer,
      role: 'developer',
      added_by: EMAILS.get(owner),
    },
  ],
  [
    DAY + 3 * HOUR,
    admin,
    'build_approved',
    'build',
    BUILD_IDS.succeeded8,
    {
      project_id: PROJECT_IDS.flutterShop,
      decided_by: EMAILS.get(admin),
      reason: 'Release notes reviewed',
    },
  ],
  [
    DAY + 3.2 * HOUR,
    developer,
    'build_approval_requested',
    'build',
    BUILD_IDS.succeeded8,
    { build_id: BUILD_IDS.succeeded8 },
  ],
  [
    DAY + 4 * HOUR,
    admin,
    'secret_rotated',
    'pipeline',
    PIPELINE_IDS.shopAndroid,
    {
      name: 'PLAY_SERVICE_ACCOUNT',
      scope: 'pipeline',
      actor: EMAILS.get(admin),
    },
  ],
  cleanup(DAY + 5 * HOUR),
  [
    DAY + 6 * HOUR,
    null,
    'retention_cleanup_completed',
    'retention_policy',
    'global',
    {
      builds_expired: 3,
      artifacts_deleted: 7,
      bytes_reclaimed: 412_300_000,
      dry_run: false,
      ran_at: ago(DAY + 6 * HOUR),
    },
  ],
  [
    DAY + 7 * HOUR,
    admin,
    'runner_renamed',
    'runner',
    RUNNER_IDS.macStudio,
    {
      previous_name: 'mac-studio-01',
      new_name: 'mac-studio-m2-ultra',
      updated_by: EMAILS.get(admin),
    },
  ],
  signIn(DAY + 8 * HOUR, qaViewer, '10.4.2.57'),
  [
    2 * DAY + 2 * HOUR,
    owner,
    'pipeline_ios_signing_updated',
    'pipeline',
    PIPELINE_IDS.shopIos,
    {
      pipeline_id: PIPELINE_IDS.shopIos,
      enabled: true,
      mode: 'automatic',
      export_method: 'app-store',
      bundle_ids: ['build.oore.fluttershop'],
      updated_by: EMAILS.get(owner),
    },
  ],
  started(
    2 * DAY + 3 * HOUR,
    developer,
    BUILD_IDS.failed1,
    PROJECT_IDS.nativePayments,
    PIPELINE_IDS.paymentsAll,
    46,
  ),
  [
    2 * DAY + 5 * HOUR,
    admin,
    'notification_channel_created',
    'notification_channel',
    NOTIFICATION_CHANNEL_IDS.slack,
    undefined,
    'type=slack, name=Releases',
  ],
  [
    3 * DAY + 1 * HOUR,
    owner,
    'update_retention_policy',
    'retention_policy',
    'global',
    {
      enabled: true,
      max_age_days: 30,
      max_builds_per_project: 50,
      cleanup_target: 'artifacts',
      dry_run: false,
    },
  ],
  [
    3 * DAY + 4 * HOUR,
    admin,
    'pipeline_created',
    'pipeline',
    PIPELINE_IDS.adminAndroid,
    {
      project_id: PROJECT_IDS.internalAdmin,
      pipeline_name: 'Debug Build',
      created_by: EMAILS.get(admin),
    },
  ],
  [
    3 * DAY + 5 * HOUR,
    owner,
    'external_access_enable_blocked',
    'instance_settings',
    'external_access',
    { failed_checks: ['public_url'], ready: false, remote_auth_mode: 'oidc' },
  ],
  [
    4 * DAY + 2 * HOUR,
    admin,
    'project_created',
    'project',
    PROJECT_IDS.nativePayments,
    { project_name: 'NativePayments', created_by: EMAILS.get(admin) },
  ],
  signIn(4 * DAY + 3 * HOUR, admin),
  [
    5 * DAY + 2 * HOUR,
    owner,
    'external_access_oidc_configured',
    'instance_settings',
    'external_access',
    { issuer_url: 'https://accounts.google.com', has_client_secret: true },
  ],
  [
    5 * DAY + 6 * HOUR,
    owner,
    'artifact_storage_updated',
    'instance_settings',
    'artifact_storage',
    { provider: 's3' },
  ],
  [
    6 * DAY + 3 * HOUR,
    owner,
    'integration_created',
    'integration',
    INTEGRATION_IDS.gitlab,
    {
      provider: 'gitlab',
      auth_mode: 'oauth_app',
      host_url: 'https://gitlab.com',
      display_name: 'GitLab',
      created_by: EMAILS.get(owner),
    },
  ],
  [
    8 * DAY + 2 * HOUR,
    owner,
    'integration_created',
    'integration',
    INTEGRATION_IDS.github,
    {
      provider: 'github',
      app_id: '482910',
      display_name: 'GitHub',
      created_by: EMAILS.get(owner),
    },
  ],
  [
    8 * DAY + 4 * HOUR,
    owner,
    'runner_registered',
    'runner',
    RUNNER_IDS.macMini,
    {
      runner_id: RUNNER_IDS.macMini,
      runner_name: 'mac-mini-m2',
      registered_by: EMAILS.get(owner),
    },
  ],
  [
    10 * DAY + 3 * HOUR,
    owner,
    'project_created',
    'project',
    PROJECT_IDS.flutterShop,
    { project_name: 'FlutterShop', created_by: EMAILS.get(owner) },
  ],
  [12 * DAY + 2 * HOUR, owner, 'owner_created', 'user', owner],
]

export const demoAuditLogs: Array<AuditLogEntry> = ROWS.map(
  (
    [secondsAgo, actor, action, resourceType, resourceId, details, note, token],
    index,
  ) => ({
    id: ROWS.length - index,
    actor_id: actor,
    actor_email: actor ? (EMAILS.get(actor) ?? null) : null,
    token_id: token ?? null,
    token_name: token ? TOKENS[token] : null,
    action,
    resource_type: resourceType,
    resource_id: resourceId,
    details: note ?? (details ? JSON.stringify(details) : null),
    created_at: ago(secondsAgo),
  }),
)
