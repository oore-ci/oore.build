import type {
  EnvironmentApprovers,
  EnvironmentApproversInput,
  EnvironmentPromotion,
  ProjectEnvironment,
  PromotionState,
  Release,
  ReleasePolicy,
} from '@oore/client/models'
import type { BadgeVariant } from '@/components/ui/badge'
import { releaseDestinationLabel } from '@/lib/releases'

export const ENVIRONMENT_KEY_PATTERN = /^[a-z][a-z0-9-]*$/
export const ENVIRONMENT_KEY_MAX_LENGTH = 32
export const ENVIRONMENT_NAME_MAX_LENGTH = 64

export const PROMOTION_STATE_LABEL = {
  pending: 'Waiting for approval',
  promoting: 'Promoting',
  completed: 'Promoted',
  rejected: 'Rejected',
  failed: 'Failed',
} satisfies Record<PromotionState, string>

const PROMOTION_STATE_VARIANT = {
  pending: 'warning',
  promoting: 'outline',
  completed: 'success',
  rejected: 'outline',
  failed: 'destructive',
} satisfies Record<PromotionState, BadgeVariant>

export function getPromotionStateVariant(state: PromotionState): BadgeVariant {
  return PROMOTION_STATE_VARIANT[state]
}

/**
 * Whether Google Play runs staged rollouts on a track. Play stages only
 * production and open testing; internal, closed and custom tracks release to
 * everyone at once, and the daemon refuses a staged policy for them.
 */
export function playTrackStagesRollouts(track: string): boolean {
  return ['production', 'open', 'beta'].includes(track.trim())
}

export function environmentTargetLabel(
  environment: Pick<ProjectEnvironment, 'store' | 'track'>,
): string {
  return releaseDestinationLabel(environment)
}

function trimPercent(fraction: number): string {
  return String(Math.round(fraction * 10_000) / 100)
}

/** `[0.01, 0.2, 1]` as `1, 20, 100`, the way the form edits steps. */
export function formatRolloutSteps(steps: ReadonlyArray<number>): string {
  return steps.map(trimPercent).join(', ')
}

/**
 * Parse rollout percentages such as `1, 5, 20, 100` into fractions. Returns
 * an error message instead when the daemon would reject them.
 */
export function parseRolloutSteps(
  text: string,
): { steps: Array<number> } | { error: string } {
  const parts = text
    .split(/[\s,]+/)
    .map((part) => part.replace(/%$/, ''))
    .filter(Boolean)
  if (parts.length === 0) return { error: 'Enter at least one percentage.' }
  const percents = parts.map(Number)
  if (percents.some((value) => !Number.isFinite(value) || value <= 0)) {
    return { error: 'Use numbers above 0, such as 1, 5, 20, 100.' }
  }
  if (percents.some((value) => value > 100)) {
    return { error: 'Percentages can be at most 100.' }
  }
  if (
    percents.some((value, index) => index > 0 && value <= percents[index - 1])
  ) {
    return { error: 'List each percentage once, from lowest to highest.' }
  }
  if (percents.at(-1) !== 100) {
    return { error: 'End at 100 so the rollout completes.' }
  }
  return { steps: percents.map((value) => value / 100) }
}

/** One line describing how releases promoted into the environment roll out. */
export function rolloutPolicySummary(
  environment: Pick<ProjectEnvironment, 'rollout_policy' | 'track'> &
    Partial<Pick<ProjectEnvironment, 'store'>>,
): string {
  const policy: ReleasePolicy = environment.rollout_policy
  const steps = policy.steps ?? [1]
  if (environment.store === 'app_store') {
    if (environment.track !== 'app_store')
      return 'TestFlight, no staged rollout'
    return steps.length > 1
      ? "Phased release on Apple's 7-day schedule"
      : 'Full release'
  }
  if (!playTrackStagesRollouts(environment.track)) return 'Full release'
  if (steps.length <= 1) return 'Full release'
  const hours = Math.round((policy.interval_hours ?? 24) * 100) / 100
  const schedule = `${formatRolloutSteps(steps).replaceAll(', ', '% → ')}% every ${hours}h`
  return policy.crash_rate_threshold == null
    ? schedule
    : `${schedule}, halts at ${trimPercent(policy.crash_rate_threshold)}% crashes`
}

/** Where a promotion starts, for one-line summaries. */
export function promotionSourceLabel(promotion: EnvironmentPromotion): string {
  return promotion.source_environment_name ?? `${promotion.source_track} track`
}

/**
 * The environment a build in `environment` usually goes to next: the next
 * one on the promotion path, in the same store, that builds can be promoted
 * into.
 */
export function nextEnvironment(
  environments: ReadonlyArray<ProjectEnvironment>,
  environment: ProjectEnvironment,
): ProjectEnvironment | undefined {
  const index = environments.findIndex((item) => item.id === environment.id)
  return environments
    .slice(index + 1)
    .find((item) => item.promotable && item.store === environment.store)
}

/**
 * Who approves promotions, as the environment dialog edits it: maintainers
 * (the default), developers and maintainers, or only named people. Named
 * people are added on top of the first two.
 */
export type ApproverMode = 'maintainer' | 'developer' | 'named'

export function approverMode(
  approvers: EnvironmentApprovers | undefined,
): ApproverMode {
  if (approvers?.role === 'developer') return 'developer'
  if (approvers?.role == null && (approvers?.users?.length ?? 0) > 0) {
    return 'named'
  }
  return 'maintainer'
}

/** The approvers to send for a mode and named user ids. */
export function approversInput(
  mode: ApproverMode,
  userIds: ReadonlyArray<string>,
): EnvironmentApproversInput {
  const user_ids = [...userIds]
  if (mode === 'named') return { user_ids }
  if (mode === 'developer') return { role: 'developer', user_ids }
  // Maintainers alone is the default; keep it stored as no approvers.
  return user_ids.length > 0 ? { role: 'maintainer', user_ids } : {}
}

/**
 * Who can approve, for a sentence: "Maintainers", "Developers and
 * maintainers, ana@example.com", or "ana@example.com, sam@example.com".
 */
export function approversSummary(
  approvers: EnvironmentApprovers | undefined,
): string {
  const emails = (approvers?.users ?? []).map((user) => user.email)
  const role =
    approvers?.role === 'developer'
      ? 'Developers and maintainers'
      : approvers?.role === 'maintainer' || emails.length === 0
        ? 'Maintainers'
        : undefined
  return [role, ...emails].filter(Boolean).join(', ')
}

/**
 * A release in this environment that can still be promoted elsewhere: a live
 * Google Play release, or a TestFlight release testers have. A release on the
 * App Store itself is the end of the path.
 */
export function promotableRelease(environment: ProjectEnvironment) {
  const release = environment.latest_release
  if (!release) return undefined
  if (release.store === 'app_store' && release.track === 'app_store') {
    return undefined
  }
  return ['rolling', 'paused', 'complete'].includes(release.state)
    ? release
    : undefined
}

/** The environment a release can be promoted from, when it is that one's. */
export function promotionSource(
  environments: ReadonlyArray<ProjectEnvironment>,
  release: Pick<Release, 'id' | 'environment_id' | 'build_id'>,
): ProjectEnvironment | undefined {
  // A release found in the store has no build to promote.
  if (!release.environment_id || !release.build_id) return undefined
  const environment = environments.find(
    (candidate) => candidate.id === release.environment_id,
  )
  return environment && promotableRelease(environment)?.id === release.id
    ? environment
    : undefined
}

/**
 * One row of the app's Releases tab: a track the app set up (an
 * environment in the API), or a store track that has releases but no setup.
 */
export interface TrackRow {
  key: string
  /** The track's own name, such as Beta, or the store track's label. */
  name: string
  store: Release['store']
  track: string
  environment?: ProjectEnvironment
  /** The newest version people can get on this track. */
  out: Release | null
  /** Newer versions on their way to this track, newest first. */
  coming: Array<Release>
  /** A promotion into this track that waits for approval. */
  pending?: EnvironmentPromotion
}

/**
 * The tracks an app releases to, in promotion order, with what is out on
 * each. Store tracks that have releases but were never set up follow.
 */
export function trackRows(
  environments: ReadonlyArray<ProjectEnvironment>,
  statuses: ReadonlyArray<Omit<TrackRow, 'name' | 'environment' | 'pending'>>,
  promotions: ReadonlyArray<EnvironmentPromotion>,
): Array<TrackRow> {
  const byKey = new Map(statuses.map((status) => [status.key, status]))
  const configured = [...environments]
    .sort((a, b) => a.position - b.position)
    .map((environment): TrackRow => {
      const key = `${environment.store}:${environment.track}`
      const status = byKey.get(key)
      byKey.delete(key)
      const latest = environment.latest_release
      const fallback =
        latest &&
        ['rolling', 'paused', 'halted', 'complete'].includes(latest.state)
          ? latest
          : null
      return {
        key,
        name: environment.name,
        store: environment.store,
        track: environment.track,
        environment,
        out: status?.out ?? fallback,
        coming: status?.coming ?? [],
        pending: promotions.find(
          (promotion) =>
            promotion.state === 'pending' &&
            promotion.environment_id === environment.id,
        ),
      }
    })
  const unconfigured = [...byKey.values()].map((status): TrackRow => ({
    ...status,
    name: releaseDestinationLabel(status),
  }))
  return [...configured, ...unconfigured]
}

export type TrackNextAction =
  | { kind: 'approve'; promotion: EnvironmentPromotion }
  | { kind: 'promote' }
  | { kind: 'release'; release: Release }
  | null

/**
 * The one action a track row offers: approve a waiting promotion, resume or
 * halt its rollout, let a store-approved version out, or promote what it has.
 * `primary` is the release's own leading action, as release controls pick it.
 */
export function trackNextAction(
  row: Pick<TrackRow, 'out' | 'coming' | 'pending'>,
  canPromote: boolean,
  primary: (release: Release) => string | null,
): TrackNextAction {
  if (row.pending?.can_decide) {
    return { kind: 'approve', promotion: row.pending }
  }
  const outAction = row.out ? primary(row.out) : null
  if (
    row.out &&
    (outAction === 'resume' ||
      outAction === 'halt' ||
      outAction === 'attach_schedule')
  ) {
    return { kind: 'release', release: row.out }
  }
  const next = row.coming[0]
  if (next && primary(next)) return { kind: 'release', release: next }
  if (canPromote) return { kind: 'promote' }
  if (row.out && outAction) return { kind: 'release', release: row.out }
  return null
}
