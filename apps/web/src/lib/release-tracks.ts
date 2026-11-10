import type { Release, ReleaseState } from '@oore/client/models'

/** States in which a version is in people's hands on a track. */
const OUT_STATES = new Set<ReleaseState>([
  'rolling',
  'paused',
  'halted',
  'complete',
])

/** States in which a version is on its way to a track. */
const COMING_STATES = new Set<ReleaseState>([
  'uploading',
  'processing',
  'blocked',
  'in_review',
  'approved',
  'rejected',
])

/** Play tracks from first testers to everyone; unknown tracks sort between. */
const PLAY_TRACK_ORDER = ['internal', 'alpha', 'closed', 'beta', 'open']

function trackRank(release: Pick<Release, 'store' | 'track'>): number {
  if (release.store === 'app_store') {
    // TestFlight groups before the App Store itself, after every Play track.
    return release.track === 'app_store' ? 300 : 200
  }
  if (release.track === 'production') return 100
  const index = PLAY_TRACK_ORDER.indexOf(release.track.toLowerCase())
  return index < 0 ? 50 : index
}

export interface TrackStatus {
  key: string
  store: Release['store']
  track: string
  /** The newest version people can get on this track. */
  out: Release | null
  /** Newer versions on their way to this track, newest first. */
  coming: Array<Release>
}

/**
 * Where each version is: one entry per store track with what is out and
 * what is coming, ordered from first testers to the public.
 */
export function releaseTracks(
  releases: ReadonlyArray<Release>,
): Array<TrackStatus> {
  const byTrack = new Map<string, Array<Release>>()
  for (const release of releases) {
    const key = `${release.store}:${release.track}`
    byTrack.set(key, [...(byTrack.get(key) ?? []), release])
  }
  return [...byTrack.entries()]
    .map(([key, list]) => {
      const newestFirst = [...list].sort((a, b) => b.created_at - a.created_at)
      const out = newestFirst.find((release) => OUT_STATES.has(release.state))
      const coming = newestFirst.filter(
        (release) =>
          COMING_STATES.has(release.state) &&
          (!out || release.created_at > out.created_at),
      )
      return {
        key,
        store: list[0].store,
        track: list[0].track,
        out: out ?? null,
        coming,
      }
    })
    .filter((track) => track.out || track.coming.length)
    .sort((a, b) => trackRank(a) - trackRank(b))
}

export interface VersionGroup {
  version: string
  /** The newest release of this version on each track, by track order. */
  releases: Array<Release>
  createdAt: number
}

/**
 * One entry per app version with its newest release on every track, so a
 * version that went to both stores reads as one release.
 */
export function releasesByVersion(
  releases: ReadonlyArray<Release>,
): Array<VersionGroup> {
  const groups = new Map<string, Map<string, Release>>()
  for (const release of releases) {
    const version = release.version_name ?? 'Unversioned'
    const tracks = groups.get(version) ?? new Map<string, Release>()
    const key = `${release.store}:${release.track}`
    const seen = tracks.get(key)
    if (!seen || release.created_at > seen.created_at) tracks.set(key, release)
    groups.set(version, tracks)
  }
  return [...groups.entries()]
    .map(([version, tracks]) => {
      const list = [...tracks.values()].sort(
        (a, b) => trackRank(a) - trackRank(b),
      )
      return {
        version,
        releases: list,
        createdAt: Math.max(...list.map((release) => release.created_at)),
      }
    })
    .sort((a, b) => b.createdAt - a.createdAt)
}

/**
 * Releases of one version the store approved and that wait for someone to
 * let them out, when there is more than one: "Release both".
 */
export function releasableTogether(
  group: Pick<VersionGroup, 'releases'>,
): Array<Release> {
  const ready = group.releases.filter(
    (release) =>
      release.state === 'approved' &&
      release.allowed_actions.includes('release_now'),
  )
  return ready.length > 1 ? ready : []
}

/**
 * When Oore would take each remaining step of a release's schedule if it
 * took over the rollout now: `[{ fraction: 0.5, at: … }, …]`.
 */
export function schedulePreview(
  release: Pick<Release, 'policy' | 'rollout_fraction'>,
  nowSeconds: number,
): Array<{ fraction: number; at: number }> {
  const hours = release.policy.interval_hours ?? 24
  return (release.policy.steps ?? [])
    .filter((fraction) => fraction > release.rollout_fraction)
    .map((fraction, index) => ({
      fraction,
      at: nowSeconds + (index + 1) * hours * 3600,
    }))
}

/**
 * The highest Google Play version code Oore has seen for an app, and
 * whether the next build's code would not be above it. Play refuses an
 * upload whose version code is not higher than every code it has.
 */
export function playVersionCodeClash(
  releases: ReadonlyArray<Pick<Release, 'store' | 'version_code'>>,
  latestBuildVersionCode: number | null | undefined,
): { highest: number; next: number } | null {
  if (latestBuildVersionCode == null) return null
  const codes = releases
    .filter((release) => release.store === 'google_play')
    .map((release) => release.version_code ?? 0)
  if (codes.length === 0) return null
  const highest = Math.max(...codes)
  const next = latestBuildVersionCode + 1
  return next <= highest ? { highest, next } : null
}
