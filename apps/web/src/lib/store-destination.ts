import type {
  AppStorePublishConfig,
  GooglePlayPublishConfig,
} from '@oore/client/models'

/** Google Play's standard tracks, in the order testers meet them. */
export const PLAY_TRACKS = {
  internal: {
    label: 'Internal testing',
    effect:
      'Up to 100 testers you list in Play Console get it within minutes. Staged rollouts are not available.',
  },
  closed: {
    label: 'Closed testing',
    effect:
      'Testers you invite by email or Google Group get it (Play’s alpha track).',
  },
  open: {
    label: 'Open testing',
    effect:
      'Anyone can join the test from your Play Store listing (Play’s beta track).',
  },
  production: {
    label: 'Production',
    effect: 'Everyone on Google Play gets it once Google approves the release.',
  },
} as const

export type PlayTrack = keyof typeof PLAY_TRACKS

export function isPlayTrack(track: string): track is PlayTrack {
  return Object.hasOwn(PLAY_TRACKS, track)
}

/** "Production" for a standard track, the name itself for a custom one. */
export function playTrackLabel(track: string) {
  return isPlayTrack(track) ? PLAY_TRACKS[track].label : track
}

type PlayDestination = Pick<
  GooglePlayPublishConfig,
  'enabled' | 'track' | 'rollout_percent' | 'draft'
>
type AppStoreDestination = Pick<
  AppStorePublishConfig,
  'enabled' | 'destination' | 'testflight_groups' | 'submit_for_review'
>

/** "Google Play Production at 5%", or null when uploads are off. */
export function playDestination(config: PlayDestination | null | undefined) {
  if (!config?.enabled) return null
  const track = isPlayTrack(config.track)
    ? `Google Play ${PLAY_TRACKS[config.track].label}`
    : `the Google Play ${config.track} track`
  if (config.draft) return `${track} as a draft`
  const percent = config.rollout_percent
  return percent != null && percent < 100 ? `${track} at ${percent}%` : track
}

/** "TestFlight (Internal QA)" or "the App Store", or null when off. */
export function appStoreDestination(
  config: AppStoreDestination | null | undefined,
) {
  if (!config?.enabled) return null
  if (config.destination === 'app_store') {
    return config.submit_for_review
      ? 'the App Store for review'
      : 'the App Store'
  }
  const groups = config.testflight_groups ?? []
  return groups.length > 0 ? `TestFlight (${groups.join(', ')})` : 'TestFlight'
}

function join(items: Array<string>) {
  if (items.length <= 1) return items.join('')
  return `${items.slice(0, -1).join(', ')} and ${items.at(-1)}`
}

/**
 * One sentence saying where a pipeline's builds go, for the pipeline page
 * and Run build. Pass only the stores the pipeline builds for.
 */
export function describeStoreUploads({
  appStore,
  googlePlay,
}: {
  appStore?: AppStoreDestination | null
  googlePlay?: PlayDestination | null
}) {
  const targets = [
    playDestination(googlePlay),
    appStoreDestination(appStore),
  ].filter((target): target is string => !!target)
  return targets.length > 0
    ? `Uploads to ${join(targets)} after each build.`
    : 'Builds are not uploaded to a store.'
}
