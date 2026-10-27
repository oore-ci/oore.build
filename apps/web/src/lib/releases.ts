import type { BadgeVariant } from '@/components/ui/badge'
import { relativeTime } from '@/lib/format-utils'
import type {
  Release,
  ReleaseAction,
  ReleaseHealth,
  ReleaseHistoryEntry,
  ReleaseState,
  ReleaseStore,
} from '@oore/client/models'

export const RELEASE_STATE_LABEL = {
  uploading: 'Uploading',
  processing: 'Processing',
  in_review: 'In review',
  rolling: 'Rolling out',
  paused: 'Paused',
  halted: 'Halted',
  complete: 'Complete',
  failed: 'Failed',
} satisfies Record<ReleaseState, string>

// Blue like a running build: the store is still working on these.
const RELEASE_STATE_VARIANT = {
  uploading: 'info',
  processing: 'info',
  in_review: 'info',
  rolling: 'secondary',
  paused: 'warning',
  halted: 'destructive',
  complete: 'success',
  failed: 'destructive',
} satisfies Record<ReleaseState, BadgeVariant>

export const RELEASE_STORE_LABEL = {
  google_play: 'Google Play',
  app_store: 'App Store',
} satisfies Record<ReleaseStore, string>

/**
 * One health vocabulary for rollouts and store vitals, so the release board
 * and Store health never describe the same numbers in different words.
 */
export const RELEASE_HEALTH_LABEL = {
  healthy: 'Healthy',
  warning: 'Near threshold',
  breached: 'Over threshold',
  unknown: 'No data',
} satisfies Record<ReleaseHealth, string>

const RELEASE_HEALTH_VARIANT = {
  healthy: 'success',
  warning: 'warning',
  breached: 'destructive',
  unknown: 'outline',
} satisfies Record<ReleaseHealth, BadgeVariant>

const TERMINAL_RELEASE_STATES = new Set<ReleaseState>(['complete', 'failed'])

/** Live releases are the ones still moving or waiting on someone. */
export const LIVE_RELEASE_STATES = [
  'uploading',
  'processing',
  'in_review',
  'rolling',
  'paused',
  'halted',
] satisfies Array<ReleaseState>

export function isLiveRelease(release: Pick<Release, 'state'>): boolean {
  return !TERMINAL_RELEASE_STATES.has(release.state)
}

const MOVING_RELEASE_STATES = new Set<ReleaseState>([
  'uploading',
  'processing',
  'in_review',
  'rolling',
])

/** Releases that change without anyone acting; halted and paused ones wait. */
export function isMovingRelease(release: Pick<Release, 'state'>): boolean {
  return MOVING_RELEASE_STATES.has(release.state)
}

const STORE_WAIT_STATES = new Set<ReleaseState>([
  'uploading',
  'processing',
  'in_review',
])

/**
 * Releases the store still holds before anyone can install them. A build
 * shows Succeeded once it uploads, so these need saying next to it.
 */
export function isWaitingOnStore(
  release: Pick<Release, 'awaiting_approval' | 'state'>,
): boolean {
  return release.awaiting_approval || STORE_WAIT_STATES.has(release.state)
}

function isTestFlight(release: Pick<Release, 'store' | 'track'>) {
  return release.store === 'app_store' && release.track !== 'app_store'
}

/** Where a release goes, in the store's words: "TestFlight · QA, Beta". */
export function releaseDestinationLabel(
  release: Pick<Release, 'store' | 'track'>,
): string {
  if (release.store === 'app_store') {
    return isTestFlight(release) ? `TestFlight · ${release.track}` : 'App Store'
  }
  return `${RELEASE_STORE_LABEL[release.store]} · ${release.track}`
}

/** One phrase for a status chip: "App Store processing". */
export function releaseStatusPhrase(
  release: Pick<Release, 'awaiting_approval' | 'state' | 'store' | 'track'>,
): string {
  const where = isTestFlight(release)
    ? 'TestFlight'
    : RELEASE_STORE_LABEL[release.store]
  const what = release.awaiting_approval
    ? 'awaiting approval'
    : RELEASE_STATE_LABEL[release.state].toLowerCase()
  return `${where} ${what}`
}

/** The steps a release passes through before people can install it. */
export function releaseStages(
  release: Pick<Release, 'store' | 'track'>,
): Array<string> {
  if (release.store === 'google_play') return ['Uploaded', 'Review', 'Rollout']
  return isTestFlight(release)
    ? ['Uploaded', 'Apple processing', 'Beta review', 'Testing']
    : ['Uploaded', 'Apple processing', 'App Store review', 'Release']
}

/** Index into `releaseStages` of the step the release is on now. */
export function releaseStageIndex(
  release: Pick<Release, 'state' | 'store'>,
): number {
  switch (release.state) {
    case 'uploading':
      return 0
    case 'processing':
      return 1
    case 'in_review':
      return release.store === 'google_play' ? 1 : 2
    default:
      return release.store === 'google_play' ? 2 : 3
  }
}

/**
 * What a release is waiting for and what happens next, for a build that
 * succeeded but whose app is not out yet. Store times are typical, not
 * promised: Apple and Google publish no processing or review SLA.
 */
export function releaseWaitSummary(
  release: Release,
): { title: string; description: string } | null {
  const version = releaseVersionLabel(release)
  const destination = releaseDestinationLabel(release)
  if (release.awaiting_approval) {
    return {
      title: 'This release is waiting for approval',
      description: `${version} goes to ${destination} once someone approves it.`,
    }
  }
  const since = relativeTime(release.updated_at)
  if (release.store === 'app_store') {
    switch (release.state) {
      case 'uploading':
        return {
          title: 'Uploading to App Store Connect',
          description: `${version} is on its way to Apple.`,
        }
      case 'processing': {
        const next = isTestFlight(release)
          ? `makes it available to ${release.track} in TestFlight`
          : `adds it to App Store version ${release.version_name ?? version}`
        return {
          title: 'Uploaded. Apple is processing the build',
          description: `Oore uploaded ${version} to App Store Connect ${relativeTime(release.created_at)}. Apple usually takes 5 to 30 minutes, sometimes a few hours. When it finishes, Oore ${next}. Oore keeps checking, so you can leave this page.`,
        }
      }
      case 'in_review':
        return isTestFlight(release)
          ? {
              title: 'Waiting for TestFlight beta review',
              description: `Apple reviews a version's first build before external testers get it, usually within a day. Submitted ${since}.`,
            }
          : {
              title: 'Waiting for App Store review',
              description: `Apple reviews most apps within a day or two. Submitted ${since}; Apple's decision shows here.`,
            }
      default:
        return null
    }
  }
  switch (release.state) {
    case 'uploading':
    case 'processing':
      return {
        title: 'Uploading to Google Play',
        description: `${version} is on its way to the ${release.track} track.`,
      }
    case 'in_review':
      return {
        title: 'Waiting for Google Play review',
        description: `Google reviews most updates within hours, sometimes a few days. Submitted ${since}; the rollout starts when Google approves it.`,
      }
    default:
      return null
  }
}

export function getReleaseStateVariant(state: ReleaseState): BadgeVariant {
  return RELEASE_STATE_VARIANT[state]
}

export function getReleaseHealthVariant(health: ReleaseHealth): BadgeVariant {
  return RELEASE_HEALTH_VARIANT[health]
}

/** From this share of its threshold a number is worth watching. */
const NEAR_THRESHOLD = 0.75

/** Mirrors the daemon's `ReleaseHealth::from_crash_rate` for any vital. */
export function healthFromThreshold(
  value: number | null | undefined,
  threshold: number | null | undefined,
): ReleaseHealth {
  if (value == null || threshold == null || threshold <= 0) return 'unknown'
  if (value >= threshold) return 'breached'
  return value >= threshold * NEAR_THRESHOLD ? 'warning' : 'healthy'
}

/** Explains a rollout's health badge: it reads this version's crash rate only. */
export function releaseHealthDetail(
  release: Pick<Release, 'crash_rate' | 'crash_rate_threshold'>,
): string {
  const threshold =
    release.crash_rate_threshold == null
      ? null
      : `${(release.crash_rate_threshold * 100).toFixed(2)}%`
  if (release.crash_rate == null) {
    return threshold
      ? `No crash data for this version yet. Rollout halts at ${threshold}.`
      : 'No crash data for this version yet.'
  }
  const rate = `${(release.crash_rate * 100).toFixed(2)}%`
  return threshold
    ? `This version's crash rate is ${rate}. Rollout halts at ${threshold}.`
    : `This version's crash rate is ${rate}.`
}

export function formatRolloutPercent(fraction: number): string {
  return `${fractionToPercent(fraction)}%`
}

/** A rollout fraction as a percentage, to two decimals. */
export function fractionToPercent(fraction: number): number {
  return Math.round(fraction * 10_000) / 100
}

/**
 * Checks a typed rollout percentage against the current rollout. Play only
 * moves a staged rollout up, so the target must be above where it is now.
 */
export function rolloutPercentError(
  value: string,
  currentFraction: number,
): string | null {
  const trimmed = value.trim()
  if (trimmed === '') return 'Enter a percentage.'
  const percent = Number(trimmed)
  if (!Number.isFinite(percent)) return 'Enter a number.'
  const current = fractionToPercent(currentFraction)
  if (percent <= current)
    return `Enter more than the current ${current}%. To stop new installs, halt the rollout.`
  if (percent > 100) return 'Enter 100% or less.'
  if (!/^\d*(\.\d{0,2})?$/.test(trimmed))
    return 'Use at most two decimal places.'
  return null
}

export function releaseVersionLabel(
  release: Pick<Release, 'version_code' | 'version_name'>,
): string {
  const name = release.version_name ?? 'Unversioned'
  return release.version_code == null
    ? name
    : `${name} (${release.version_code})`
}

/** Release rows can predate their project lookup; never render "null". */
export function releaseAppName(release: { project_name?: string | null }) {
  return release.project_name ?? 'Unknown app'
}

/** One line telling the reader what happens next without opening the release. */
export function releaseNextStep(release: Release, nowSeconds: number): string {
  if (release.awaiting_approval) return 'Waiting for approval'
  switch (release.state) {
    case 'uploading':
      return 'Uploading to the store'
    case 'processing':
      return release.store === 'app_store'
        ? 'Apple is processing the build'
        : 'Store is processing'
    case 'in_review':
      return release.store === 'app_store'
        ? 'Waiting for Apple review'
        : 'Waiting for store review'
    case 'paused':
      return `Held at ${formatRolloutPercent(release.rollout_fraction)} until resumed`
    case 'halted':
      return 'Stopped for new users until resumed'
    case 'complete':
      return 'Live for all users'
    case 'failed':
      return 'Release failed'
    case 'rolling': {
      if (release.next_fraction == null) return 'Waiting to complete'
      const target = formatRolloutPercent(release.next_fraction)
      if (release.next_step_at == null) return `Next: ${target}`
      return `${target} ${formatRelativeFuture(release.next_step_at, nowSeconds)}`
    }
  }
}

function formatRelativeFuture(at: number, nowSeconds: number): string {
  const seconds = at - nowSeconds
  if (seconds <= 60) return 'any moment'
  if (seconds < 3600) return `in ${Math.round(seconds / 60)}m`
  if (seconds < 86_400) return `in ${Math.round(seconds / 3600)}h`
  return `in ${Math.round(seconds / 86_400)}d`
}

/** QA-facing phrasing of where a release stands. */
export function releaseAudienceLabel(release: Release): string {
  if (release.state === 'complete') return 'Live for all users'
  if (release.state === 'rolling' || release.state === 'paused') {
    return release.track === 'production'
      ? `Live for ${formatRolloutPercent(release.rollout_fraction)} of users`
      : `In ${release.track} testing`
  }
  if (release.state === 'halted') return 'Rollout halted'
  if (release.state === 'failed') return 'Release failed'
  return `Submitted to ${RELEASE_STORE_LABEL[release.store]}`
}

export const RELEASE_ACTION_ORDER = [
  'approve',
  'raise',
  'resume',
  'pause',
  'complete',
  'halt',
] satisfies Array<ReleaseAction>

interface ReleaseActionCopy {
  label: string
  done: string
  title: string
  description: (release: Release) => string
  confirmLabel: string
  destructive?: boolean
}

export const RELEASE_ACTION_COPY = {
  approve: {
    done: 'Release approved',
    label: 'Approve',
    title: 'Approve release?',
    description: (release) =>
      `${releaseVersionLabel(release)} continues to ${RELEASE_STORE_LABEL[release.store]} ${release.track}.`,
    confirmLabel: 'Approve',
  },
  raise: {
    done: 'Rollout updated',
    label: 'Update rollout',
    title: 'Update rollout?',
    description: (release) =>
      `${releaseVersionLabel(release)} is live for ${formatRolloutPercent(release.rollout_fraction)} of ${release.track} users. Choose the share that gets it now.`,
    confirmLabel: 'Update rollout',
  },
  resume: {
    done: 'Rollout resumed',
    label: 'Resume',
    title: 'Resume rollout?',
    description: (release) =>
      `${releaseVersionLabel(release)} continues its rollout schedule from ${formatRolloutPercent(release.rollout_fraction)}.`,
    confirmLabel: 'Resume',
  },
  pause: {
    done: 'Rollout paused',
    label: 'Pause',
    title: 'Pause rollout?',
    description: (release) =>
      `${releaseVersionLabel(release)} stays at ${formatRolloutPercent(release.rollout_fraction)} and stops advancing until resumed.`,
    confirmLabel: 'Pause',
  },
  complete: {
    done: 'Released to everyone',
    label: 'Complete rollout',
    title: 'Release to everyone?',
    description: (release) =>
      `${releaseVersionLabel(release)} goes to 100% of ${release.track} users.`,
    confirmLabel: 'Release to 100%',
  },
  halt: {
    done: 'Rollout halted',
    label: 'Halt',
    title: 'Halt rollout?',
    description: (release) => {
      const stops = `${RELEASE_STORE_LABEL[release.store]} stops serving ${releaseVersionLabel(release)} to new users. Users who already updated keep it.`
      const percent = formatRolloutPercent(release.rollout_fraction)
      return release.state === 'paused'
        ? `Paused only holds the rollout at ${percent}; new users still get this version. ${stops}`
        : `${stops} To hold the rollout at ${percent} instead, pause it.`
    },
    confirmLabel: 'Halt rollout',
    destructive: true,
  },
} satisfies Record<ReleaseAction, ReleaseActionCopy>

/** Short description of what changed in one history entry. */
export function releaseTransitionLabel(
  entry: Pick<
    ReleaseHistoryEntry,
    'from_fraction' | 'from_state' | 'to_fraction' | 'to_state'
  >,
): string {
  if (
    entry.to_fraction != null &&
    entry.from_fraction != null &&
    entry.to_fraction !== entry.from_fraction
  ) {
    return `${formatRolloutPercent(entry.from_fraction)} → ${formatRolloutPercent(entry.to_fraction)}`
  }
  if (!entry.from_state) return 'Created'
  if (entry.from_state === entry.to_state) return ''
  return `from ${RELEASE_STATE_LABEL[entry.from_state].toLowerCase()}`
}
