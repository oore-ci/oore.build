import type {
  EnvironmentApprovers,
  EnvironmentApproversInput,
  EnvironmentPromotion,
  ProjectEnvironment,
  PromotionState,
  ReleasePolicy,
} from '@oore/client/models'
import type { BadgeVariant } from '@/components/ui/badge'
import { RELEASE_STORE_LABEL } from '@/lib/releases'

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

/** Play track names as Play Console shows them. */
const PLAY_TRACK_LABEL = new Map([
  ['internal', 'Internal testing'],
  ['closed', 'Closed testing'],
  ['open', 'Open testing'],
  ['production', 'Production'],
])

export function environmentTargetLabel(
  environment: Pick<ProjectEnvironment, 'store' | 'track'>,
): string {
  const track =
    environment.store === 'google_play'
      ? (PLAY_TRACK_LABEL.get(environment.track) ?? environment.track)
      : environment.track
  return `${RELEASE_STORE_LABEL[environment.store]} · ${track}`
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
  if (environment.track === 'internal') return 'Full release'
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
