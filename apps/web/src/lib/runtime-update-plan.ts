export interface RuntimeComponentRelease {
  version: string
  latest_version: string
  update_available: boolean
  managed_service: boolean
}

export interface RuntimeComponentHealth {
  version?: string
}

export type CoordinatedUpdatePlan =
  | {
      kind: 'ready'
      target: string
      backend: boolean
      frontend: boolean
    }
  | { kind: 'unavailable'; reason: string }

export function normalizeVersion(raw: string | undefined | null) {
  return raw?.trim().replace(/^v/, '') ?? ''
}

/**
 * True when a backend readiness report comes from the target release: the
 * installed `VERSION` names the full release and the running binary carries
 * its `X.Y.Z` package version, which pre-releases share.
 */
export function backendRunsRelease(
  ready: { version?: string; package_version?: string },
  target: string,
) {
  return (
    normalizeVersion(ready.version) === target &&
    normalizeVersion(ready.package_version) === target.replace(/-.*$/, '')
  )
}

/**
 * A release check is keyed by the version reported when it ran, so it can
 * outlive the update it describes. Only offer it while the latest health
 * response still reports an older version.
 */
export function releaseIsOffered(
  release: RuntimeComponentRelease | undefined,
  health: RuntimeComponentHealth | undefined,
) {
  if (release?.update_available !== true) return false
  return (
    normalizeVersion(health?.version) !==
    normalizeVersion(release.latest_version)
  )
}

const VERSION_PATTERN = /^(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta)\.(\d+))?$/
function preReleaseRank(channel: string | undefined) {
  if (channel === 'alpha') return 0
  if (channel === 'beta') return 1
  return 2
}

/**
 * Orders two Oore release versions: negative when `a` is older. Versions
 * that do not parse compare equal only to themselves.
 */
export function compareReleaseVersions(a: string, b: string) {
  const left = VERSION_PATTERN.exec(normalizeVersion(a))
  const right = VERSION_PATTERN.exec(normalizeVersion(b))
  if (!left || !right) {
    return normalizeVersion(a).localeCompare(normalizeVersion(b))
  }
  for (const index of [1, 2, 3]) {
    const diff = Number(left[index]) - Number(right[index])
    if (diff !== 0) return diff
  }
  const rank = preReleaseRank(left[4]) - preReleaseRank(right[4])
  if (rank !== 0) return rank
  return Number(left[5] ?? 0) - Number(right[5] ?? 0)
}

/**
 * Oore ships the frontend and backend from one release, so a combined update
 * must leave both components on the same version.
 *
 * The backend decides the target: its updater installs the newest release,
 * and the frontend is then pinned to whatever the backend runs. The
 * frontend's own release check is not used for the target, because it can
 * be older or newer than the backend's while releases are being published.
 * A frontend left behind the backend (an update cut short) gets a catch-up
 * update instead of a refusal.
 */
export function planCoordinatedUpdate({
  frontend,
  backend,
  frontendOffered,
  backendOffered,
  backendManaged,
  backendUnavailableReason,
}: {
  frontend: RuntimeComponentRelease | undefined
  backend: RuntimeComponentRelease | undefined
  frontendOffered: boolean
  backendOffered: boolean
  backendManaged: boolean
  /** The failing update preflight check, when one is known. */
  backendUnavailableReason?: string
}): CoordinatedUpdatePlan {
  if (!frontend || !backend) {
    return {
      kind: 'unavailable',
      reason:
        'This deployment serves the frontend and backend separately. Update each component below.',
    }
  }

  const target = normalizeVersion(
    backendOffered ? backend.latest_version : backend.version,
  )
  const frontendVersion = normalizeVersion(frontend.version)
  const frontendOrder = compareReleaseVersions(frontendVersion, target)
  const frontendNeeded = frontendOrder < 0

  if (frontendOrder > 0) {
    return {
      kind: 'unavailable',
      reason: `The frontend runs ${frontendVersion}, which is newer than the backend's ${target}. Run oore update in Terminal on the backend's machine.`,
    }
  }
  if (!backendOffered && !frontendNeeded) {
    return {
      kind: 'unavailable',
      reason: frontendOffered
        ? `${normalizeVersion(frontend.latest_version)} is published, but the backend has not seen it yet. Check again in a few minutes.`
        : 'Oore is up to date.',
    }
  }
  if (backendOffered && !backendManaged) {
    return {
      kind: 'unavailable',
      reason:
        backendUnavailableReason ??
        'The backend is not running as a managed service. Run Oore setup from Terminal to repair it.',
    }
  }
  if (frontendNeeded && !frontend.managed_service) {
    return {
      kind: 'unavailable',
      reason:
        'The frontend is not running as a managed service. Install oore-web as a managed service to update it here.',
    }
  }

  return {
    kind: 'ready',
    target,
    backend: backendOffered,
    frontend: frontendNeeded,
  }
}

/**
 * True when the frontend runs an older release than the backend, which an
 * interrupted update can leave behind.
 */
export function frontendBehindBackend(
  frontend: RuntimeComponentHealth | undefined,
  backend: RuntimeComponentHealth | undefined,
) {
  if (!frontend?.version || !backend?.version) return false
  return compareReleaseVersions(frontend.version, backend.version) < 0
}
