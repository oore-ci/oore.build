import type { AuditLogEntry } from '@oore/client/models'
import * as z from 'zod'

import {
  formatAbsoluteTime,
  formatEnumLabel,
  formatFileSize,
} from '@/lib/format-utils'

/** One piece of an audit sentence. `strong` marks a name. */
export interface AuditPart {
  strong?: boolean
  text: string
}

export interface AuditFact {
  label: string
  mono?: boolean
  value: string
}

export interface AuditDescription {
  /** Who did it: a person, `Oore` for the server, `Someone` when unknown. */
  actor: string
  actorKind: 'person' | 'system' | 'unknown'
  /** Labelled details for the expanded row. */
  facts: Array<AuditFact>
  /** One short line of the facts that matter most, when there are any. */
  meta: string | null
  /** Set when the entry records something blocked or failed. */
  problem: 'Blocked' | 'Failed' | null
  /** The words after the actor: "created an install link for …". */
  say: Array<AuditPart>
}

/** One value from an entry's details, once parsed. */
export type AuditValue =
  | { kind: 'text'; text: string }
  | { kind: 'number'; number: number }
  | { kind: 'flag'; on: boolean }
  | { kind: 'list'; items: Array<string> }
  | { kind: 'other'; json: string }

type Details = ReadonlyMap<string, AuditValue>

export interface ParsedAuditDetails {
  fields: Details
  /** Details that are plain words rather than fields. */
  note: string | null
}

const valueSchema = z.union([
  z.string().transform((text): AuditValue => ({ kind: 'text', text })),
  z.number().transform((number): AuditValue => ({ kind: 'number', number })),
  z.boolean().transform((on): AuditValue => ({ kind: 'flag', on })),
  z.array(z.union([z.string(), z.number()])).transform((items): AuditValue => ({
    kind: 'list',
    items: items.map(String),
  })),
  z.null(),
  z.json().transform((value): AuditValue => ({
    kind: 'other',
    json: JSON.stringify(value),
  })),
])

const detailsSchema = z.record(z.string(), valueSchema)

const ROLES = new Map([
  ['owner', 'Owner'],
  ['admin', 'Admin'],
  ['developer', 'Developer'],
  ['qa_viewer', 'QA Viewer'],
  ['maintainer', 'Maintainer'],
  ['viewer', 'Viewer'],
])

const PROPER_WORDS = new Map([
  ['api', 'API'],
  ['github', 'GitHub'],
  ['gitlab', 'GitLab'],
  ['ios', 'iOS'],
  ['oidc', 'OIDC'],
  ['qa', 'QA'],
  ['smtp', 'SMTP'],
  ['url', 'URL'],
  ['ip', 'IP'],
  ['id', 'ID'],
])

const STORES = new Map([
  ['app_store', 'the App Store'],
  ['google_play', 'Google Play'],
])

function label(value: string) {
  return formatEnumLabel(value.replace(/[.-]+/g, '_'))
    .split(' ')
    .map((word) => PROPER_WORDS.get(word.toLowerCase()) ?? word)
    .join(' ')
}

/** A field as words: text as written, a number as digits. */
function text(d: Details, key: string): string | undefined {
  const value = d.get(key)
  if (value?.kind === 'text') return value.text.trim() || undefined
  if (value?.kind === 'number') return String(value.number)
  return undefined
}

/** A field as an amount: a number, or how many items a list holds. */
function count(d: Details, key: string): number | undefined {
  const value = d.get(key)
  if (value?.kind === 'number') return value.number
  if (value?.kind === 'list') return value.items.length
  return undefined
}

function flag(d: Details, key: string): boolean | undefined {
  const value = d.get(key)
  return value?.kind === 'flag' ? value.on : undefined
}

function items(d: Details, key: string): Array<string> | undefined {
  const value = d.get(key)
  return value?.kind === 'list' ? value.items : undefined
}

function plural(amount: number, noun: string) {
  if (amount === 1) return `1 ${noun}`
  return `${amount} ${noun === 'person' ? 'people' : `${noun}s`}`
}

function shortDate(epochSecs: number) {
  return new Date(epochSecs * 1000).toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
  })
}

function roleLabel(key: string | undefined) {
  return key ? (ROLES.get(key) ?? label(key)) : undefined
}

function role(d: Details, key: string) {
  return roleLabel(text(d, key))
}

/** A field that holds an enum value such as `oauth_app`, as words. */
function labelled(d: Details, key: string) {
  const value = text(d, key)
  return value ? label(value) : undefined
}

function joined(parts: Array<string | undefined | null | false>) {
  const kept = parts.filter((part): part is string => Boolean(part))
  return kept.length > 0 ? kept.join(' · ') : null
}

/**
 * Details are JSON for most actions. A few older actions write
 * `name=ci, role=developer` instead, which reads as the same fields.
 */
export function parseAuditDetails(
  details: string | null | undefined,
): ParsedAuditDetails {
  const raw = details?.trim()
  if (!raw) return { fields: new Map(), note: null }
  const fields = new Map<string, AuditValue>()
  try {
    const parsed = detailsSchema.safeParse(JSON.parse(raw))
    if (parsed.success) {
      for (const [key, value] of Object.entries(parsed.data)) {
        if (value) fields.set(key, value)
      }
      return { fields, note: null }
    }
  } catch {
    // Not JSON: fall through to the older formats.
  }
  const pairs = raw.split(/,\s*/).map((pair) => /^(\w+)=(.*)$/.exec(pair))
  for (const pair of pairs) {
    if (!pair) return { fields: new Map(), note: raw }
    fields.set(pair[1], { kind: 'text', text: pair[2] })
  }
  return { fields, note: null }
}

/** Everything a sentence template may need about one entry. */
interface Context {
  d: Details
  entry: AuditLogEntry
  /** The current name of a record, when the server still knows it. */
  name: (id: string | null | undefined) => string | undefined
}

const strong = (value: string): AuditPart => ({ text: value, strong: true })
const plain = (value: string): AuditPart => ({ text: value })

/** A name when there is one, otherwise plain words such as "a pipeline". */
function named(value: string | undefined, fallback: string): AuditPart {
  return value ? strong(value) : plain(fallback)
}

function build({ entry }: Context): AuditPart {
  if (entry.build_number == null) return plain('a build')
  return strong(
    [entry.project_name, entry.pipeline_name, `#${entry.build_number}`]
      .filter(Boolean)
      .join(' '),
  )
}

function pipeline({ d, entry, name }: Context): AuditPart {
  const own =
    entry.pipeline_name ?? name(entry.resource_id) ?? text(d, 'pipeline_name')
  if (!own) return plain('a pipeline')
  return strong([entry.project_name, own].filter(Boolean).join(' '))
}

function storeName(d: Context['d']) {
  return text(d, 'store') === 'app_store' ? 'App Store Connect' : 'Google Play'
}

function app({ d, entry }: Context): AuditPart {
  return named(entry.project_name ?? text(d, 'project_name'), 'an app')
}

function resource(context: Context, fallback: string, key = 'name'): AuditPart {
  const { d, entry, name } = context
  return named(name(entry.resource_id) ?? text(d, key), fallback)
}

function person(
  { name }: Context,
  id: string | null | undefined,
  fallback = 'a user',
) {
  return named(name(id), fallback)
}

function cleanupMeta(d: Details, extra: Array<string | false>) {
  const freed = count(d, 'bytes_reclaimed') ?? 0
  const facts = [...extra, freed > 0 && `${formatFileSize(freed)} freed`]
  const summary = joined(facts)
  return joined([
    flag(d, 'dry_run') === true && 'Dry run',
    summary ?? 'Nothing to delete',
  ])
}

function amount(d: Details, key: string, noun: string) {
  const total = count(d, key) ?? 0
  return total > 0 && plural(total, noun)
}

function percentOf(fraction: number) {
  return `${Math.round(fraction * 1000) / 10}%`
}

function percent(d: Details, key: string) {
  const value = count(d, key)
  return value == null ? undefined : percentOf(value)
}

function fromTo(d: Details, from: string, to: string) {
  const before = text(d, from)
  const after = text(d, to)
  return before && after ? `${label(before)} to ${label(after)}` : null
}

interface TemplateResult {
  meta?: string | null
  say: Array<AuditPart>
}

type Template = (context: Context) => TemplateResult

const release =
  (verb: string, meta?: (d: Details) => string | null): Template =>
  (c) => ({
    say: [plain(`${verb} the release of `), build(c)],
    meta: meta ? meta(c.d) : joined([text(c.d, 'reason')]),
  })

const promotion =
  (verb: string): Template =>
  (c) => ({
    say: [
      plain(`${verb} `),
      build(c),
      plain(' to '),
      named(text(c.d, 'environment'), 'a track'),
    ],
    meta: joined([text(c.d, 'reason') ?? text(c.d, 'error')]),
  })

const onPipeline =
  (words: string, meta?: (d: Details) => string | null): Template =>
  (c) => ({ say: [plain(`${words} `), pipeline(c)], meta: meta?.(c.d) })

const onResource =
  (words: string, fallback: string, key?: string): Template =>
  (c) => ({ say: [plain(`${words} `), resource(c, fallback, key)] })

const secret =
  (verb: string): Template =>
  (c) => {
    const scope = text(c.d, 'scope')
    const where =
      scope === 'pipeline' ? pipeline(c) : scope === 'project' ? app(c) : null
    return {
      say: [
        plain(`${verb} the secret `),
        named(text(c.d, 'name'), 'a secret'),
        ...(where ? [plain(' in '), where] : []),
      ],
      meta: scope === 'instance' ? 'Shared by every app' : null,
    }
  }

const simple =
  (words: string, meta?: (d: Details) => string | null): Template =>
  (c) => ({ say: [plain(words)], meta: meta?.(c.d) })

const TEMPLATES = new Map<string, Template>(
  Object.entries({
    // Access
    trusted_proxy_login_succeeded: simple('signed in', (d) =>
      joined(['Through the proxy', text(d, 'peer_ip')]),
    ),
    trusted_proxy_login_blocked_untrusted_peer: simple(
      'tried to sign in from an address that is not a trusted proxy',
      (d) => joined([text(d, 'peer_ip')]),
    ),
    local_recovery_login_succeeded: simple('signed in with a recovery link'),
    local_recovery_login_failed: simple(
      'failed to sign in with a recovery link',
      (d) => joined([labelled(d, 'reason')]),
    ),
    local_recovery_capability_minted: simple('created a recovery sign-in link'),
    local_recovery_capability_mint_failed: simple(
      'could not create a recovery sign-in link',
      (d) => joined([labelled(d, 'reason')]),
    ),
    local_operator_request_failed: simple(
      'refused a command from the local operator',
      (d) => joined([labelled(d, 'reason')]),
    ),
    owner_created: simple('became the owner of this instance'),
    owner_created_auto_local: simple('became the owner of this instance'),
    owner_backfilled: simple('became the owner of this instance'),
    user_invited: (c) => ({
      say: [
        plain('invited '),
        named(text(c.d, 'email') ?? c.name(c.entry.resource_id), 'a user'),
        ...(role(c.d, 'role') ? [plain(` as ${role(c.d, 'role')}`)] : []),
      ],
    }),
    user_activated: simple('accepted their invitation'),
    role_changed: (c) => ({
      say: [
        plain('changed '),
        person(c, c.entry.resource_id),
        ...(role(c.d, 'from_role') && role(c.d, 'to_role')
          ? [
              plain(
                ` from ${role(c.d, 'from_role')} to ${role(c.d, 'to_role')}`,
              ),
            ]
          : [plain("'s role")]),
      ],
    }),
    user_disabled: (c) => ({
      say: [plain('disabled '), person(c, c.entry.resource_id)],
      meta: joined([
        amount(c.d, 'revoked_sessions', 'session') &&
          `${amount(c.d, 'revoked_sessions', 'session')} ended`,
        amount(c.d, 'revoked_api_tokens', 'API token') &&
          `${amount(c.d, 'revoked_api_tokens', 'API token')} revoked`,
      ]),
    }),
    user_enabled: (c) => ({
      say: [plain('enabled '), person(c, c.entry.resource_id), plain(' again')],
    }),
    api_token_created: (c) => ({
      say: [plain('created the API token '), resource(c, 'a token')],
      meta: joined([
        role(c.d, 'role') && `${role(c.d, 'role')} role`,
        text(c.d, 'prefix') && `Starts with ${text(c.d, 'prefix')}`,
      ]),
    }),
    api_token_revoked: onResource('revoked the API token', 'a token'),
    project_member_added: (c) => ({
      say: [
        plain('added '),
        person(c, text(c.d, 'added_user_id')),
        plain(' to '),
        app(c),
        ...(role(c.d, 'role') ? [plain(` as ${role(c.d, 'role')}`)] : []),
      ],
    }),
    project_member_role_updated: (c) => ({
      say: [
        plain('made '),
        person(c, text(c.d, 'target_user_id')),
        plain(` ${role(c.d, 'new_role') ?? 'a new role'} in `),
        app(c),
      ],
    }),
    project_member_removed: (c) => ({
      say: [
        plain('removed '),
        person(c, text(c.d, 'removed_user_id')),
        plain(' from '),
        app(c),
      ],
    }),
    project_store_connected: (c) => ({
      say: [plain(`connected ${storeName(c.d)} to `), app(c)],
    }),
    project_store_disconnected: (c) => ({
      say: [plain(`disconnected ${storeName(c.d)} from `), app(c)],
    }),
    project_store_tested: (c) => ({
      say: [plain(`tested the ${storeName(c.d)} connection of `), app(c)],
      meta:
        flag(c.d, 'ok') === true
          ? 'Passed'
          : flag(c.d, 'ok') === false
            ? 'Failed'
            : undefined,
    }),

    // Builds and releases
    build_created: (c) => ({ say: [plain('started '), build(c)] }),
    build_rerun: (c) => ({ say: [plain('reran a build as '), build(c)] }),
    build_step_retry: (c) => ({
      say: [plain('retried failed steps as '), build(c)],
      meta: items(c.d, 'retry_steps')?.join(', ') ?? null,
    }),
    build_canceled: (c) => ({
      say: [plain('canceled '), build(c)],
      meta: text(c.d, 'from_status') ? `Was ${text(c.d, 'from_status')}` : null,
    }),
    build_approval_requested: (c) => ({
      say: [plain('asked for approval to run '), build(c)],
    }),
    build_approved: (c) => ({
      say: [plain('approved '), build(c)],
      meta: joined([text(c.d, 'reason')]),
    }),
    build_rejected: (c) => ({
      say: [plain('rejected '), build(c)],
      meta: joined([text(c.d, 'reason')]),
    }),
    build_sent_to_testers: (c) => ({
      say: [plain('sent '), build(c), plain(' to testers')],
      meta: joined([
        c.name(text(c.d, 'group_id')),
        amount(c.d, 'recipients', 'person'),
      ]),
    }),
    build_kept: (c) => ({
      say: [plain('kept '), build(c), plain(' from cleanup')],
    }),
    build_unkept: (c) => ({ say: [plain('stopped keeping '), build(c)] }),
    store_upload_completed: (c) => ({
      say: [plain('uploaded '), build(c), plain(' to the store')],
    }),
    store_upload_record_failed: (c) => ({
      say: [
        plain('uploaded '),
        build(c),
        plain(' but could not record the release'),
      ],
      meta: joined([text(c.d, 'error')]),
    }),
    artifact_install_link_generated: (c) => ({
      say: [plain('created an install link for '), build(c)],
      meta: joined([
        labelled(c.d, 'platform'),
        count(c.d, 'expires_at') != null &&
          `Expires ${shortDate(count(c.d, 'expires_at')!)}`,
      ]),
    }),
    scoped_download_token_created: (c) => ({
      say: [plain('created a download link for '), build(c)],
      meta: joined([
        flag(c.d, 'single_use') === true && 'Works once',
        count(c.d, 'expires_at') != null &&
          `Expires ${shortDate(count(c.d, 'expires_at')!)}`,
      ]),
    }),
    scoped_download_token_revoked: (c) => ({
      say: [plain('revoked a download link for '), build(c)],
    }),
    artifact_download_link_generated: (c) => ({
      say: [plain('created a download link for '), build(c)],
      meta: joined([c.name(text(c.d, 'artifact_id'))]),
    }),
    latest_install_link_created: onPipeline(
      'created a latest-build install link for',
      (d) => joined([labelled(d, 'platform'), text(d, 'branch')]),
    ),
    latest_install_link_deleted: onPipeline(
      'deleted a latest-build install link for',
    ),
    ios_device_self_registered: (c) => ({
      say: [
        plain('registered the device '),
        named(text(c.d, 'device_name'), 'a device'),
        plain(' for '),
        pipeline(c),
      ],
      meta: joined([text(c.d, 'product')]),
    }),
    pipeline_ios_device_registered: (c) => ({
      say: [
        plain('registered the device '),
        named(text(c.d, 'device_name'), 'a device'),
        plain(' for '),
        pipeline(c),
      ],
    }),
    release_created: (c) => ({
      say: [
        plain('released '),
        build(c),
        plain(` to ${STORES.get(text(c.d, 'store') ?? '') ?? 'the store'}`),
      ],
      meta: joined([
        text(c.d, 'track') && `${labelled(c.d, 'track')} track`,
        percent(c.d, 'rollout_fraction'),
      ]),
    }),
    release_approved: release('approved'),
    release_raised: release('raised', (d) =>
      joined([percent(d, 'to_fraction') && `To ${percent(d, 'to_fraction')}`]),
    ),
    release_advanced: release('advanced', (d) =>
      joined([percent(d, 'to_fraction') && `To ${percent(d, 'to_fraction')}`]),
    ),
    release_paused: release('paused'),
    release_resumed: release('resumed'),
    release_halted: release('halted'),
    release_completed: release('completed'),
    release_retried: release('rechecked'),
    release_released_now: release('published'),
    release_resubmitted: release('resubmitted'),
    release_pulled_from_review: release('pulled from review'),
    release_marked_sent_for_review: release('marked as sent for review'),
    release_marked_rejected: release('marked as rejected'),
    release_synced_from_store: release('synced from the store', (d) =>
      fromTo(d, 'from_state', 'to_state'),
    ),
    release_schedule_attached: release('scheduled the rollout for'),
    release_schedule_detached: release('stopped the scheduled rollout for'),
    release_freeze_set: (c) => ({
      say:
        c.entry.resource_type === 'project'
          ? [plain('froze releases for '), app(c)]
          : [plain('froze releases for every app')],
      meta: text(c.d, 'reason') ? `Reason: ${text(c.d, 'reason')}` : null,
    }),
    release_freeze_lifted: (c) => ({
      say:
        c.entry.resource_type === 'project'
          ? [plain('lifted the release freeze for '), app(c)]
          : [plain('lifted the release freeze for every app')],
    }),
    release_notes_updated: (c) => ({
      say: [plain('updated the release notes of '), app(c)],
      meta: text(c.d, 'version_name')
        ? `Version ${text(c.d, 'version_name')}`
        : null,
    }),
    promotion_requested: promotion('asked to promote'),
    promotion_approved: promotion('approved promoting'),
    promotion_rejected: promotion('rejected promoting'),
    promotion_completed: promotion('promoted'),
    promotion_failed: promotion('could not promote'),

    // Settings
    project_created: (c) => ({ say: [plain('created the app '), app(c)] }),
    project_updated: (c) => ({ say: [plain('updated the app '), app(c)] }),
    project_deleted: simple('deleted an app'),
    pipeline_created: (c) => ({
      say: [plain('created the pipeline '), pipeline(c)],
      meta: text(c.d, 'config_path')
        ? `From ${text(c.d, 'config_path')}`
        : null,
    }),
    pipeline_updated: onPipeline('updated the pipeline'),
    pipeline_deleted: (c) => ({
      say: [plain('deleted a pipeline')],
      meta: joined([
        amount(c.d, 'artifacts_deleted', 'artifact') &&
          `${amount(c.d, 'artifacts_deleted', 'artifact')} deleted`,
      ]),
    }),
    pipeline_app_store_publish_updated: onPipeline(
      'updated App Store publishing for',
      (d) => joined([flag(d, 'enabled') === false && 'Turned off']),
    ),
    pipeline_google_play_publish_updated: onPipeline(
      'updated Google Play publishing for',
      (d) =>
        joined([
          flag(d, 'enabled') === false && 'Turned off',
          text(d, 'track') && `${labelled(d, 'track')} track`,
        ]),
    ),
    pipeline_google_play_credentials_tested: onPipeline(
      'tested the Google Play credentials of',
      (d) =>
        flag(d, 'ok') === true
          ? 'Passed'
          : flag(d, 'ok') === false
            ? 'Did not pass'
            : null,
    ),
    pipeline_android_signing_updated: onPipeline('updated Android signing for'),
    pipeline_ios_signing_updated: onPipeline('updated iOS signing for'),
    pipeline_ios_signing_sync: onPipeline('synced iOS signing for', (d) =>
      joined([
        amount(d, 'updated_profiles', 'profile') &&
          `${amount(d, 'updated_profiles', 'profile')} updated`,
      ]),
    ),
    environment_created: (c) => ({
      say: [
        plain('created the track '),
        named(text(c.d, 'key'), 'a track'),
        plain(' in '),
        app(c),
      ],
    }),
    environment_updated: (c) => ({
      say: [
        plain('updated the track '),
        named(text(c.d, 'key'), 'a track'),
        plain(' in '),
        app(c),
      ],
    }),
    environment_deleted: (c) => ({
      say: [
        plain('deleted the track '),
        named(text(c.d, 'key'), 'a track'),
        plain(' from '),
        app(c),
      ],
    }),
    secret_created: secret('added'),
    secret_rotated: secret('rotated'),
    secret_deleted: secret('deleted'),
    app_store_connect_key_created: onResource(
      'added the App Store Connect key',
      'a key',
    ),
    app_store_connect_key_updated: onResource(
      'updated the App Store Connect key',
      'a key',
    ),
    app_store_connect_key_deleted: onResource(
      'deleted the App Store Connect key',
      'a key',
    ),
    integration_created: (c) => ({
      say: [
        plain('connected the source '),
        resource(c, 'a source', 'display_name'),
      ],
      meta: joined([labelled(c.d, 'provider')]),
    }),
    integration_deleted: onResource(
      'removed the source',
      'a source',
      'display_name',
    ),
    integration_credential_replaced: onResource(
      'replaced the credential of the source',
      'a source',
    ),
    gitlab_repository_webhook_secret_rotated: (c) => ({
      say: [
        plain('rotated the webhook secret of '),
        named(text(c.d, 'repository_full_name'), 'a repository'),
      ],
    }),
    runner_registered: onResource(
      'added the runner',
      'a runner',
      'runner_name',
    ),
    runner_renamed: (c) => ({
      say: [
        plain('renamed the runner '),
        named(text(c.d, 'previous_name'), 'a runner'),
        plain(' to '),
        named(
          text(c.d, 'new_name') ?? c.name(c.entry.resource_id),
          'a new name',
        ),
      ],
    }),
    runner_labels_updated: (c) => ({
      say: [
        plain('changed the labels of the runner '),
        resource(c, 'a runner'),
      ],
      meta: joined([
        items(c.d, 'new_labels') &&
          (items(c.d, 'new_labels')?.join(', ') || 'No labels'),
      ]),
    }),
    runner_concurrency_updated: (c) => ({
      say: [
        plain('changed how many builds the runner '),
        resource(c, 'a runner'),
        plain(' runs at once'),
      ],
      meta: fromTo(
        c.d,
        'previous_max_concurrent_builds',
        'new_max_concurrent_builds',
      ),
    }),
    runner_cache_purge_requested: onResource(
      'asked for the caches to be cleared on the runner',
      'a runner',
    ),
    notification_channel_created: onResource(
      'added the notification channel',
      'a channel',
    ),
    notification_channel_updated: onResource(
      'updated the notification channel',
      'a channel',
    ),
    notification_channel_deleted: onResource(
      'deleted the notification channel',
      'a channel',
    ),
    tester_group_created: onResource('created the tester group', 'a group'),
    tester_group_updated: onResource('updated the tester group', 'a group'),
    tester_group_deleted: onResource('deleted the tester group', 'a group'),
    update_retention_policy: simple('updated the cleanup policy', (d) =>
      joined([
        flag(d, 'enabled') === false && 'Turned off',
        count(d, 'max_age_days') != null &&
          `Keep ${plural(count(d, 'max_age_days')!, 'day')}`,
      ]),
    ),
    update_project_retention: (c) => ({
      say: [plain('set cleanup rules for '), app(c)],
    }),
    delete_project_retention: (c) => ({
      say: [plain('removed the cleanup rules of '), app(c)],
    }),
    artifact_storage_updated: simple(
      'changed where artifacts are stored',
      (d) => joined([labelled(d, 'provider')]),
    ),
    external_access_trusted_proxy_settings_updated: simple(
      'updated the trusted proxy settings',
    ),
    external_access_network_settings_updated: simple(
      'updated the external access addresses',
      (d) => joined([text(d, 'public_url')]),
    ),
    external_access_preflight_checked: simple(
      'checked that external access is ready',
      (d) =>
        flag(d, 'ready') === true
          ? 'All checks passed'
          : amount(d, 'failed_checks', 'check')
            ? `${amount(d, 'failed_checks', 'check')} did not pass`
            : null,
    ),
    external_access_oidc_configured: simple('set up single sign-on', (d) =>
      joined([text(d, 'issuer_url')]),
    ),
    external_access_enable_blocked: simple(
      'tried to turn on external access',
      (d) =>
        amount(d, 'failed_checks', 'check')
          ? `${amount(d, 'failed_checks', 'check')} did not pass`
          : null,
    ),
    external_access_enabled: simple('turned on external access'),
    instance_preferences_updated: simple('updated the instance preferences'),
    direct_macos_runner_pause_updated: (c) => ({
      say: [
        flag(c.d, 'direct_macos_runner_paused') === true
          ? plain('paused builds on this Mac')
          : plain('resumed builds on this Mac'),
      ],
    }),
    runtime_mode_changed: simple('changed the runtime mode', (d) =>
      fromTo(d, 'from_mode', 'to_mode'),
    ),
    remote_auth_mode_changed: simple('changed how people sign in', (d) =>
      fromTo(d, 'from_remote_auth_mode', 'to_remote_auth_mode'),
    ),
    runtime_update_started: simple('started an update of Oore'),

    // Housekeeping
    retention_cleanup_completed: simple('cleaned up old builds', (d) =>
      cleanupMeta(d, [
        amount(d, 'builds_expired', 'build') &&
          `${amount(d, 'builds_expired', 'build')} expired`,
        amount(d, 'artifacts_deleted', 'artifact') &&
          `${amount(d, 'artifacts_deleted', 'artifact')} deleted`,
      ]),
    ),
    artifact_expiry_cleanup_completed: simple(
      'cleaned up expired artifacts',
      (d) =>
        cleanupMeta(d, [
          amount(d, 'artifacts_deleted', 'artifact') &&
            `${amount(d, 'artifacts_deleted', 'artifact')} deleted`,
          amount(d, 'tokens_deleted', 'install link') &&
            `${amount(d, 'tokens_deleted', 'install link')} deleted`,
        ]),
    ),
  }),
)

/** Actions without a template still read as a sentence: "paused release". */
function fallbackSay(action: string): Array<AuditPart> {
  const words = action.split(/[_.-]+/).filter(Boolean)
  const last = words.at(-1) ?? ''
  if (words.length > 1 && /ed$/.test(last)) {
    const rest = label(words.slice(0, -1).join('_')).toLowerCase()
    return [plain(`${last} ${rest}`)]
  }
  return [plain(`did: ${label(action).toLowerCase()}`)]
}

export function auditProblem(action: string): AuditDescription['problem'] {
  if (/blocked|not_allowed/.test(action)) return 'Blocked'
  if (/failed/.test(action)) return 'Failed'
  return null
}

/** Details that only repeat the actor, or ids the row already names. */
const HIDDEN_FACTS = new Set([
  'actor',
  'actor_email',
  'ran_at',
  'build_id',
  'pipeline_id',
  'project_id',
  'runner_id',
  'link_id',
  'capability_id',
  'build_number',
  'pipeline_name',
  'project_name',
])

const FACT_LABELS = new Map(
  Object.entries({
    added_user_id: 'User',
    target_user_id: 'User',
    removed_user_id: 'User',
    group_id: 'Tester group',
    artifact_id: 'Artifact',
    release_id: 'Release',
    source_release_id: 'Source release',
    source_build_id: 'Source build',
    peer_ip: 'IP address',
    token_prefix: 'Token starts with',
    prefix: 'Token starts with',
    expires_at: 'Expires',
    bytes_reclaimed: 'Space freed',
    ttl_secs: 'Valid for',
    from_role: 'Role before',
    to_role: 'Role after',
    new_role: 'Role',
  }),
)

const MONO_FACTS = new Set([
  'peer_ip',
  'token_prefix',
  'prefix',
  'udid',
  'file_path',
  'config_path',
  'repository_path',
  'issuer_url',
  'public_url',
  'host_url',
  'artifact_delivery_url',
  'package_name',
])

const ENUM_FACT =
  /(_type|_mode|_status|_state|_method|^provider|^platform|^store|^track|^state|^scope|^destination|^trigger|^channel|^reason)$/

function textFact(key: string, value: string, context: Context) {
  if (/role$/.test(key)) return roleLabel(value) ?? value
  if (ENUM_FACT.test(key) && /^[a-z0-9_]+$/.test(value)) return label(value)
  return context.name(value) ?? value
}

function numberFact(key: string, value: number) {
  if (key.endsWith('_at')) return formatAbsoluteTime(value)
  if (key.startsWith('bytes_') || key.endsWith('_bytes')) {
    return formatFileSize(value)
  }
  if (key.endsWith('_fraction')) return percentOf(value)
  if (key === 'ttl_secs') return `${Math.round(value / 60)} min`
  return String(value)
}

function factValue(key: string, value: AuditValue, context: Context): string {
  switch (value.kind) {
    case 'flag':
      return value.on ? 'Yes' : 'No'
    case 'number':
      return numberFact(key, value.number)
    case 'text':
      return textFact(key, value.text, context)
    case 'list':
      return value.items.length > 0
        ? value.items.map((item) => textFact(key, item, context)).join(', ')
        : 'None'
    case 'other':
      return value.json
  }
}

function facts(context: Context, note: string | null): Array<AuditFact> {
  const { d, entry } = context
  const list: Array<AuditFact> = []
  if (entry.project_name) list.push({ label: 'App', value: entry.project_name })
  if (entry.pipeline_name) {
    list.push({ label: 'Pipeline', value: entry.pipeline_name })
  }
  if (entry.build_number != null) {
    list.push({ label: 'Build', value: `#${entry.build_number}` })
  }
  for (const [key, value] of d) {
    if (value.kind === 'text' && value.text === '') continue
    if (HIDDEN_FACTS.has(key) || /_by$/.test(key)) continue
    list.push({
      label: FACT_LABELS.get(key) ?? label(key),
      value: factValue(key, value, context),
      mono: MONO_FACTS.has(key) || value.kind === 'other',
    })
  }
  if (note) list.push({ label: 'Details', value: note })
  list.push({ label: 'Recorded', value: formatAbsoluteTime(entry.created_at) })
  list.push({ label: 'Action', value: entry.action, mono: true })
  if (entry.resource_id) {
    list.push({ label: 'Record ID', value: entry.resource_id, mono: true })
  }
  return list
}

export function describeAuditEntry(entry: AuditLogEntry): AuditDescription {
  const { fields, note } = parseAuditDetails(entry.details)
  const names = entry.names ?? {}
  const context: Context = {
    d: fields,
    entry,
    name: (id) => (id ? names[id] : undefined),
  }
  const problem = auditProblem(entry.action)
  const actor = entry.actor_name ?? entry.actor_email
  const template = TEMPLATES.get(entry.action)
  const described = template?.(context)
  return {
    actor:
      actor ??
      (entry.actor_id ? 'A removed user' : problem ? 'Someone' : 'Oore'),
    actorKind:
      actor || entry.actor_id ? 'person' : problem ? 'unknown' : 'system',
    say: described?.say ?? fallbackSay(entry.action),
    meta: described?.meta ?? (template ? null : note),
    problem,
    facts: facts(context, note),
  }
}

/** Up to two letters for an avatar: "Alex Chen" and "alex.chen@…" give AC. */
export function auditInitials(actor: string) {
  const words = actor
    .split('@')[0]
    .split(/[\s._+-]+/)
    .filter(Boolean)
  const letters =
    words.length > 1 ? `${words[0][0]}${words.at(-1)![0]}` : actor.slice(0, 2)
  return letters.toUpperCase()
}

export interface AuditRow {
  entry: AuditLogEntry
  /** Later runs of the same housekeeping on the same day, newest first. */
  repeats: Array<AuditLogEntry>
}

export interface AuditDay {
  /** Local calendar day, `yyyy-MM-dd`. */
  date: string
  rows: Array<AuditRow>
}

export function localDay(epochSecs: number) {
  const date = new Date(epochSecs * 1000)
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${date.getFullYear()}-${month}-${day}`
}

/**
 * Entries in the order given, split by local day. Housekeeping the server
 * repeats through a day folds into its first row, so three cleanups read as
 * one line with a count.
 */
export function groupAuditEntries(
  entries: Array<AuditLogEntry>,
): Array<AuditDay> {
  const days: Array<AuditDay> = []
  for (const entry of entries) {
    const date = localDay(entry.created_at)
    let day = days.at(-1)
    if (day?.date !== date) {
      day = { date, rows: [] }
      days.push(day)
    }
    const first =
      entry.category === 'system' && !entry.actor_id
        ? day.rows.find(
            (row) => row.entry.action === entry.action && !row.entry.actor_id,
          )
        : undefined
    if (first) first.repeats.push(entry)
    else day.rows.push({ entry, repeats: [] })
  }
  return days
}

/** "Today", "Yesterday", then the weekday and date. */
export function auditDayLabel(
  date: string,
  today = localDay(Date.now() / 1000),
) {
  const day = new Date(`${date}T00:00:00`)
  const full = day.toLocaleDateString(undefined, {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
    year:
      day.getFullYear() === Number(today.slice(0, 4)) ? undefined : 'numeric',
  })
  const yesterday = new Date(`${today}T00:00:00`)
  yesterday.setDate(yesterday.getDate() - 1)
  if (date === today) return `Today, ${full}`
  if (date === localDay(yesterday.getTime() / 1000)) return `Yesterday, ${full}`
  return full
}
