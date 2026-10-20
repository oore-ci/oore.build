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

/**
 * Oore ships the frontend and backend from one release, so a combined update
 * must leave both components on the same version.
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
  if (!frontendOffered && !backendOffered) {
    return { kind: 'unavailable', reason: 'Oore is up to date.' }
  }

  const frontendTarget = normalizeVersion(
    frontendOffered ? frontend.latest_version : frontend.version,
  )
  const backendTarget = normalizeVersion(
    backendOffered ? backend.latest_version : backend.version,
  )
  if (frontendTarget !== backendTarget) {
    return {
      kind: 'unavailable',
      reason: `The frontend would end on ${frontendTarget} and the backend on ${backendTarget}. Oore only updates matching releases together.`,
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
  if (frontendOffered && !frontend.managed_service) {
    return {
      kind: 'unavailable',
      reason:
        'The frontend is not running as a managed service. Install oore-web as a managed service to update it here.',
    }
  }

  return {
    kind: 'ready',
    target: frontendTarget,
    backend: backendOffered,
    frontend: frontendOffered,
  }
}
