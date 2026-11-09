import type { BadgeVariant } from '@/components/ui/badge'
import { relativeTime } from '@/lib/format-utils'
import type {
  Release,
  ReleaseAction,
  ReleaseActivityEvent,
  ReleaseHealth,
  ReleaseHistoryEntry,
  ReleaseState,
  ReleaseStore,
} from '@oore/client/models'

export const RELEASE_STATE_LABEL = {
  uploading: 'Uploading',
  processing: 'Processing',
  blocked: 'Blocked',
  in_review: 'In review',
  approved: 'Approved',
  rejected: 'Rejected',
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
  // Someone has to act in the store; nothing moves until they do.
  blocked: 'warning',
  in_review: 'info',
  approved: 'info',
  rejected: 'destructive',
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
  'blocked',
  'in_review',
  'approved',
  'rejected',
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
  'blocked',
  'in_review',
  'approved',
  'rejected',
])

/**
 * Releases the store still holds before anyone can install them. A build
 * shows Succeeded once it uploads, so these need saying next to it.
 */
export function isWaitingOnStore(
  release: Pick<Release, 'awaiting_approval' | 'state'>,
): boolean {
  return (
    release.awaiting_approval ||
    STORE_WAIT_STATES.has(release.state) ||
    releaseStoreError(release) !== null
  )
}

/**
 * The store's last refusal while Oore moves a live release forward, such as
 * App Store Connect blocking a review submission. Oore keeps retrying until
 * it clears; a refusal also blocks the release.
 */
export function releaseStoreError(
  release: Pick<Release, 'state' | 'store_error' | 'store_error_since'>,
): { message: string; since: number | null } | null {
  if (!release.store_error || !isLiveRelease(release)) return null
  return {
    message: release.store_error,
    since: release.store_error_since ?? null,
  }
}

/**
 * The store's own word for where a live release is, in App Store Connect's
 * casing: `WAITING_FOR_REVIEW` reads "Waiting for review". Oore's in_review
 * covers both waiting and being reviewed, and people wait differently for
 * each.
 */
export function releaseStoreStatus(
  release: Pick<Release, 'state' | 'store_status'>,
): { code: string; label: string } | null {
  const code = release.store_status
  if (!code || !isLiveRelease(release)) return null
  const words = code.toLowerCase().split('_').filter(Boolean)
  if (!words.length) return null
  const label = words.join(' ')
  return { code, label: label[0].toUpperCase() + label.slice(1) }
}

/** The status a badge shows: the store's own while it reviews. */
export function releaseStateLabel(
  release: Pick<Release, 'awaiting_approval' | 'state'>,
): string {
  if (release.state === 'blocked') return RELEASE_STATE_LABEL.blocked
  if (releaseStoreError(release)) return 'Needs attention'
  if (release.awaiting_approval) return 'Awaiting approval'
  const status = releaseStoreStatus(release)
  if (release.state === 'in_review' && status) return status.label
  return RELEASE_STATE_LABEL[release.state]
}

export function releaseStateBadgeVariant(
  release: Pick<Release, 'awaiting_approval' | 'state'>,
): BadgeVariant {
  if (release.state === 'rejected') return 'destructive'
  if (releaseStoreError(release) || release.awaiting_approval) return 'warning'
  return RELEASE_STATE_VARIANT[release.state]
}

/** Releases someone has to act on in the store before anything moves. */
export function needsStoreAction(release: Pick<Release, 'state'>): boolean {
  return release.state === 'blocked' || release.state === 'rejected'
}

/** Where to fix or follow the release: App Store Connect or Play Console. */
export function releaseConsoleLink(
  release: Pick<Release, 'console_url' | 'store'>,
): { href: string; label: string } | null {
  if (!release.console_url) return null
  return {
    href: release.console_url,
    label:
      release.store === 'app_store'
        ? 'Open in App Store Connect'
        : 'Open in Play Console',
  }
}

/** A release found in the store rather than uploaded by Oore. */
export function isAdoptedRelease(release: Pick<Release, 'origin'>): boolean {
  return release.origin === 'adopted'
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
  return `${where} ${releaseStateLabel(release).toLowerCase()}`
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
    case 'blocked':
    case 'in_review':
    case 'rejected':
      return release.store === 'google_play' ? 1 : 2
    case 'approved':
      return release.store === 'google_play' ? 2 : 3
    default:
      return release.store === 'google_play' ? 2 : 3
  }
}

export interface ReleaseWaitSummary {
  title: string
  description: string
  /** The store refused a step and someone has to fix it there. */
  blocked?: boolean
}

/**
 * What a release is waiting for and what happens next, for a build that
 * succeeded but whose app is not out yet. Store times are typical, not
 * promised: Apple and Google publish no processing or review SLA.
 */
export function releaseWaitSummary(
  release: Release,
): ReleaseWaitSummary | null {
  const version = releaseVersionLabel(release)
  const destination = releaseDestinationLabel(release)
  const storeError = releaseStoreError(release)
  const consoleName =
    release.store === 'app_store' ? 'App Store Connect' : 'Play Console'
  if (release.state === 'rejected') {
    return {
      title: `${release.store === 'app_store' ? 'Apple' : 'Google'} rejected ${version}`,
      description: `${release.state_reason ?? 'Read why in ' + consoleName + '.'} Fix it there, then resubmit.`,
      blocked: true,
    }
  }
  if (release.state === 'blocked' || storeError) {
    // A store error is shown as it came; the summary says what Oore does.
    const description = storeError
      ? `Fix it in ${consoleName}. Oore checks again every 5 minutes${storeError.since ? `, and has been since ${relativeTime(storeError.since)}` : ''}, and carries on once it is accepted.`
      : (release.state_reason ?? `${consoleName} is waiting on you.`)
    return {
      title: `${version} is blocked in ${consoleName}`,
      description,
      blocked: true,
    }
  }
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
          ? appleBetaReviewSummary(release, since)
          : appleReviewSummary(release, since)
      case 'approved':
        return {
          title: 'Approved by Apple',
          description:
            release.store_status === 'PENDING_DEVELOPER_RELEASE'
              ? `${version} passed review and waits for you to release it. Use Release now, or release it in App Store Connect.`
              : `${version} passed review. Apple puts it on the App Store on the release date you chose, or within a day.`,
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

function appleReviewSummary(
  release: Release,
  since: string,
): ReleaseWaitSummary {
  switch (releaseStoreStatus(release)?.code) {
    case 'IN_REVIEW':
      return {
        title: 'Apple is reviewing the app',
        description: `Review has started; most finish within a day. Apple's decision shows here.`,
      }
    case 'PENDING_DEVELOPER_RELEASE':
      return {
        title: 'Approved by Apple',
        description: `${releaseVersionLabel(release)} passed review. Release it in App Store Connect to put it on the App Store.`,
      }
    case 'READY_FOR_REVIEW':
    case 'PREPARE_FOR_SUBMISSION':
      return {
        title: 'Not submitted for review yet',
        description: `The build is attached to the version, but Apple has no review request. Submit it in App Store Connect.`,
      }
    default:
      return {
        title: 'Waiting for App Store review',
        description: `Submitted ${since}. Apple hasn't started reviewing yet; most apps are reviewed within a day or two. Apple's decision shows here.`,
      }
  }
}

function appleBetaReviewSummary(
  release: Release,
  since: string,
): ReleaseWaitSummary {
  if (releaseStoreStatus(release)?.code === 'IN_BETA_REVIEW') {
    return {
      title: 'Apple is reviewing the TestFlight build',
      description: `Review has started; external testers get it once Apple approves it.`,
    }
  }
  return {
    title: 'Waiting for TestFlight beta review',
    description: `Apple reviews a version's first build before external testers get it, usually within a day. Submitted ${since}.`,
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
  if (release.state === 'blocked') {
    return release.store === 'app_store'
      ? 'Fix it in App Store Connect'
      : 'Send it for review in Play Console'
  }
  if (release.state === 'rejected') return 'Fix and resubmit'
  if (release.state === 'approved') {
    return release.store_status === 'PENDING_DEVELOPER_RELEASE'
      ? 'Approved; release it when ready'
      : 'Approved; releasing soon'
  }
  if (releaseStoreError(release)) {
    return release.store === 'app_store'
      ? 'Fix the issue in App Store Connect'
      : 'Fix the issue in the store'
  }
  if (release.awaiting_approval) return 'Waiting for approval'
  switch (release.state) {
    case 'uploading':
      return 'Uploading to the store'
    case 'processing':
      return release.store === 'app_store'
        ? 'Apple is processing the build'
        : 'Store is processing'
    case 'in_review':
      if (release.store !== 'app_store') return 'Waiting for store review'
      switch (releaseStoreStatus(release)?.code) {
        case 'IN_REVIEW':
        case 'IN_BETA_REVIEW':
          return 'Apple is reviewing it'
        case 'PENDING_DEVELOPER_RELEASE':
          return 'Release it in App Store Connect'
        case 'READY_FOR_REVIEW':
        case 'PREPARE_FOR_SUBMISSION':
          return 'Submit it for review in App Store Connect'
        default:
          return "In Apple's queue; review hasn't started"
      }
    case 'paused':
      return `Held at ${formatRolloutPercent(release.rollout_fraction)} until resumed`
    case 'halted':
      return 'Stopped for new users until resumed'
    case 'complete':
      return 'Live for all users'
    case 'failed':
      return 'Release failed'
    case 'rolling': {
      if (release.store === 'google_play' && release.auto_rollout === false)
        return 'No schedule; raise it when ready'
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
  if (release.state === 'blocked') return 'Not submitted yet'
  if (release.state === 'rejected') return 'Rejected in review'
  if (release.state === 'approved') return 'Approved, not live yet'
  return `Submitted to ${RELEASE_STORE_LABEL[release.store]}`
}

export const RELEASE_ACTION_ORDER = [
  'approve',
  'release_now',
  'resubmit',
  'retry',
  'mark_sent_for_review',
  'raise',
  'resume',
  'pause',
  'complete',
  'attach_schedule',
  'detach_schedule',
  'pull_from_review',
  'mark_rejected',
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
      `${releaseVersionLabel(release)} continues to ${releaseDestinationLabel(release).replace(' · ', ' ')}.`,
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
  retry: {
    done: 'Checking the store again',
    label: 'Check again now',
    title: 'Check the store again?',
    description: (release) =>
      `Oore reads ${release.store === 'app_store' ? 'App Store Connect' : 'the store'} for ${releaseVersionLabel(release)} now instead of at its next check.`,
    confirmLabel: 'Check now',
  },
  release_now: {
    done: 'Release requested',
    label: 'Release now',
    title: 'Release on the App Store now?',
    description: (release) =>
      `Apple approved ${releaseVersionLabel(release)}. It goes on the App Store once Apple processes the request.`,
    confirmLabel: 'Release now',
  },
  resubmit: {
    done: 'Resubmitted for review',
    label: 'Resubmit for review',
    title: 'Resubmit for review?',
    description: (release) =>
      `${releaseVersionLabel(release)} goes back to App Review with the same build. Fix what Apple asked for in App Store Connect first; reply in the Resolution Center if needed.`,
    confirmLabel: 'Resubmit',
  },
  pull_from_review: {
    done: 'Pulled from review',
    label: 'Pull from review',
    title: 'Pull from review?',
    description: (release) =>
      `${releaseVersionLabel(release)} leaves App Review and goes back to waiting for a submission. You can submit it again later.`,
    confirmLabel: 'Pull from review',
    destructive: true,
  },
  mark_sent_for_review: {
    done: 'Marked as sent for review',
    label: 'Mark sent for review',
    title: 'Did you send it for review?',
    description: (release) =>
      `Play's API doesn't tell Oore when a held change is sent. Confirm you pressed Send for review in Play Console for ${releaseVersionLabel(release)}; Oore then follows the rollout.`,
    confirmLabel: 'Yes, it was sent',
  },
  mark_rejected: {
    done: 'Marked as rejected',
    label: 'Mark rejected',
    title: 'Did Google reject it?',
    description: (release) =>
      `Records that Google Play rejected ${releaseVersionLabel(release)}. Play Console has the reason.`,
    confirmLabel: 'Mark rejected',
    destructive: true,
  },
  attach_schedule: {
    done: 'Schedule attached',
    label: 'Let Oore roll it out',
    title: 'Let Oore roll it out?',
    description: (release) =>
      `Oore takes over ${releaseVersionLabel(release)}'s rollout from ${formatRolloutPercent(release.rollout_fraction)} with the release's schedule${release.policy.steps?.length ? ` (${release.policy.steps.map(formatRolloutPercent).join(', ')})` : ''}, halting on crash spikes.`,
    confirmLabel: 'Attach schedule',
  },
  detach_schedule: {
    done: 'Schedule detached',
    label: 'Stop scheduled steps',
    title: 'Stop scheduled steps?',
    description: (release) =>
      `${releaseVersionLabel(release)} stays at ${formatRolloutPercent(release.rollout_fraction)} until someone raises it, here or in Play Console.`,
    confirmLabel: 'Stop steps',
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

/** An app version's newest change, standing for every change in the window. */
export type ReleaseLatestChange = {
  event: ReleaseActivityEvent
  /** Changes to this version, across its tracks, among the events read. */
  changes: number
  /** Where the version went, newest change first: "production", "beta". */
  tracks: string[]
}

function compareVersionsDesc(
  a: string | null | undefined,
  b: string | null | undefined,
) {
  return (b ?? '').localeCompare(a ?? '', undefined, { numeric: true })
}

/**
 * One entry per app version and store, newest change first, so a phased
 * rollout's steps and the same build on several tracks read as one row.
 * Changes at the same moment list the newer version first.
 */
export function latestChangePerRelease(
  events: ReadonlyArray<ReleaseActivityEvent>,
  limit: number,
): ReleaseLatestChange[] {
  const byVersion = new Map<string, ReleaseLatestChange>()
  for (const event of events) {
    const key = `${event.project_id}:${event.store}:${event.version_name ?? event.release_id}`
    const seen = byVersion.get(key)
    if (!seen) {
      byVersion.set(key, { event, changes: 1, tracks: [event.track] })
      continue
    }
    seen.changes += 1
    if (!seen.tracks.includes(event.track)) seen.tracks.push(event.track)
    if (event.at > seen.event.at) seen.event = event
  }
  return [...byVersion.values()]
    .sort(
      (a, b) =>
        b.event.at - a.event.at ||
        compareVersionsDesc(a.event.version_name, b.event.version_name) ||
        releaseAppName(a.event).localeCompare(releaseAppName(b.event)),
    )
    .slice(0, limit)
}

/** Where an app version went: "Google Play · production, beta". */
export function releaseChangeDestination(change: ReleaseLatestChange): string {
  const { event, tracks } = change
  if (event.store === 'google_play') {
    return `${RELEASE_STORE_LABEL.google_play} · ${tracks.join(', ')}`
  }
  return [
    ...new Set(
      tracks.map((track) =>
        releaseDestinationLabel({ store: event.store, track }),
      ),
    ),
  ].join(', ')
}

/** Where a release stands after a change: "50%" while rolling, else nothing. */
export function releaseChangeShare(
  event: Pick<ReleaseActivityEvent, 'to_fraction' | 'to_state'>,
): string | null {
  const partial =
    event.to_state === 'rolling' ||
    event.to_state === 'paused' ||
    event.to_state === 'halted'
  return partial && event.to_fraction != null && event.to_fraction < 1
    ? formatRolloutPercent(event.to_fraction)
    : null
}
