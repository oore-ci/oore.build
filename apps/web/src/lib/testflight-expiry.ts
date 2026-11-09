import type { Release } from '@oore/client/models'

/** Apple stops a TestFlight build 90 days after upload. */
export const TESTFLIGHT_DAYS = 90
const DAY = 86_400

export interface TestflightExpiry {
  release: Pick<
    Release,
    | 'id'
    | 'project_id'
    | 'project_name'
    | 'track'
    | 'version_name'
    | 'version_code'
  >
  expiresAt: number
  /** Two weeks out, the last three days, or already stopped. */
  band: 'fortnight' | 'days' | 'expired'
}

function isTestflight(release: Pick<Release, 'store' | 'track'>) {
  return release.store === 'app_store' && release.track !== 'app_store'
}

/**
 * The newest TestFlight build of each app and group that testers lose
 * within two weeks, or lost in the last week. An older build in the same
 * group is already replaced, so it never warns.
 */
export function testflightExpiries(
  releases: ReadonlyArray<Release>,
  nowSeconds: number,
): Array<TestflightExpiry> {
  const newest = new Map<string, Release>()
  for (const release of releases) {
    if (!isTestflight(release) || release.state === 'failed') continue
    const key = `${release.project_id}\n${release.track}`
    const seen = newest.get(key)
    if (!seen || release.created_at > seen.created_at) newest.set(key, release)
  }
  const found: Array<TestflightExpiry> = []
  for (const release of newest.values()) {
    const expiresAt = release.created_at + TESTFLIGHT_DAYS * DAY
    const left = expiresAt - nowSeconds
    if (left > 14 * DAY || left < -7 * DAY) continue
    found.push({
      release,
      expiresAt,
      band: left <= 0 ? 'expired' : left <= 3 * DAY ? 'days' : 'fortnight',
    })
  }
  return found.sort((a, b) => a.expiresAt - b.expiresAt)
}
