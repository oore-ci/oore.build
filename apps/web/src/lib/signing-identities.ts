import type {
  IosExportMethod,
  PipelineWorkflowSettings,
  SigningIdentity,
  SigningIdentityKind,
  SigningIdentityUse,
  UpdatePipelineWorkflowSettingsRequest,
} from '@oore/client/models'

/** Names follow workflow names, so a file can say `signing: { ios: coin-store }`. */
export const SIGNING_IDENTITY_NAME = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/

export const SIGNING_IDENTITY_KIND_LABELS = {
  android_keystore: 'Android keystore',
  ios_distribution: 'iOS distribution',
} satisfies Record<SigningIdentityKind, string>

export const IOS_EXPORT_METHOD_LABELS = {
  ad_hoc: 'Ad hoc',
  app_store: 'App Store',
} satisfies Record<IosExportMethod, string>

function plural(count: number, one: string, many = `${one}s`) {
  return `${count} ${count === 1 ? one : many}`
}

/** One line on what an identity holds, without its secrets. */
export function signingIdentitySummary(identity: SigningIdentity): string {
  if (identity.kind === 'android_keystore') {
    return [
      identity.filename,
      identity.key_alias ? `alias ${identity.key_alias}` : null,
    ]
      .filter(Boolean)
      .join(' · ')
  }
  const profiles = identity.profiles ?? []
  return [
    identity.export_method
      ? IOS_EXPORT_METHOD_LABELS[identity.export_method]
      : null,
    identity.team_id ? `Team ${identity.team_id}` : null,
    profiles.length > 0 ? plural(profiles.length, 'profile') : null,
  ]
    .filter(Boolean)
    .join(' · ')
}

/** `Multi-platform Release (release)` for one use. */
export function signingIdentityUseLabel(use: SigningIdentityUse): string {
  return `${use.pipeline_name} (${use.workflow})`
}

/** Where the identity is picked, for its row and its delete prompt. */
export function signingIdentityUsage(identity: SigningIdentity): string {
  const uses = identity.used_by ?? []
  if (uses.length === 0) return 'Not picked in any workflow settings'
  return `Picked by ${uses.map(signingIdentityUseLabel).join(', ')}`
}

/**
 * The soonest expiry of the certificate and its profiles, so a row can warn
 * before builds start failing.
 */
export function signingIdentityExpiry(
  identity: SigningIdentity,
): number | undefined {
  const dates = [
    identity.expires_at,
    ...(identity.profiles ?? []).map((profile) => profile.expires_at),
  ].filter((value): value is number => value != null)
  return dates.length > 0 ? Math.min(...dates) : undefined
}

const DAY_SECONDS = 86_400

/** `expired`, `soon` (within 30 days) or `ok`, at `now` in Unix seconds. */
export function expiryState(
  expiresAt: number,
  now = Math.floor(Date.now() / 1000),
): 'expired' | 'soon' | 'ok' {
  if (expiresAt <= now) return 'expired'
  if (expiresAt - now <= 30 * DAY_SECONDS) return 'soon'
  return 'ok'
}

// ── Per-workflow settings form ───────────────────────────────────

/** Select value for "follow the file, then the pipeline". */
export const FOLLOW_FILE = '__file__'

export type ApprovalChoice = 'follow' | 'required' | 'not_required'

export interface WorkflowSettingsFormValues {
  android_identity_id: string
  ios_identity_id: string
  publish_enabled: boolean
  approval: ApprovalChoice
}

export function workflowSettingsFormValues(
  settings?: PipelineWorkflowSettings,
): WorkflowSettingsFormValues {
  return {
    android_identity_id: settings?.android_identity_id ?? FOLLOW_FILE,
    ios_identity_id: settings?.ios_identity_id ?? FOLLOW_FILE,
    publish_enabled: settings?.publish_enabled !== false,
    approval:
      settings?.approval_required === true
        ? 'required'
        : settings?.approval_required === false
          ? 'not_required'
          : 'follow',
  }
}

/**
 * The PUT body. The request replaces every field, so publishing that is on
 * is left unset (follow the file and the Publish settings) and only `false`
 * is stored.
 */
export function workflowSettingsRequest(
  values: WorkflowSettingsFormValues,
): UpdatePipelineWorkflowSettingsRequest {
  const request: UpdatePipelineWorkflowSettingsRequest = {}
  if (values.android_identity_id !== FOLLOW_FILE) {
    request.android_identity_id = values.android_identity_id
  }
  if (values.ios_identity_id !== FOLLOW_FILE) {
    request.ios_identity_id = values.ios_identity_id
  }
  if (!values.publish_enabled) request.publish_enabled = false
  if (values.approval !== 'follow') {
    request.approval_required = values.approval === 'required'
  }
  return request
}

/** True when the workflow keeps no settings, so everything follows the file. */
export function isDefaultWorkflowSettings(
  settings?: PipelineWorkflowSettings,
): boolean {
  return (
    !settings ||
    (!settings.android_identity_id &&
      !settings.ios_identity_id &&
      settings.publish_enabled == null &&
      settings.approval_required == null)
  )
}
