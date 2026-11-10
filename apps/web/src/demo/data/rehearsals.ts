import type { BuildRehearsalResponse } from '@oore/client/models'
import { BUILD_IDS } from '../seed'

/** What the demo's rehearsal builds found. */
export const demoRehearsals = new Map<string, BuildRehearsalResponse>([
  [
    BUILD_IDS.rehearsalFailed,
    {
      build_id: BUILD_IDS.rehearsalFailed,
      stores: ['google_play', 'app_store'],
      checks: [
        {
          store: 'app_store',
          code: 'age_rating',
          level: 'block',
          title: 'Age rating questions are unanswered',
          detail: 'Unanswered: gambling, contests.',
          console_url:
            'https://appstoreconnect.apple.com/apps/1234567890/distribution/info',
          remembered: false,
        },
        {
          store: 'app_store',
          code: 'review_details',
          level: 'block',
          title: 'App Review contact details are missing',
          detail: 'Missing: contact phone, demo account password.',
          console_url:
            'https://appstoreconnect.apple.com/apps/1234567890/distribution/ios/version/inflight',
          remembered: false,
        },
        {
          store: 'google_play',
          code: 'play_release_notes',
          level: 'warn',
          title: 'No release notes',
          detail:
            'Users see no "What\'s new" text. Write notes when you start the build, or add them to the repository.',
          remembered: false,
        },
        {
          store: 'app_store',
          code: 'asc_validate',
          level: 'ok',
          title: 'App Store Connect accepts the signed IPA',
          detail: 'No errors validating archive. Validated, not uploaded.',
          remembered: false,
        },
        {
          store: 'google_play',
          code: 'play_version_code',
          level: 'ok',
          title: 'Version code 48 is free on Google Play',
          detail: 'The highest on production is 44.',
          remembered: false,
        },
        {
          store: 'google_play',
          code: 'play_validate',
          level: 'ok',
          title: 'Google Play accepts 3.3.0 (48) on production',
          detail:
            'Staged and checked by Google Play, then discarded. Nothing was committed.',
          remembered: false,
        },
      ],
    },
  ],
  [
    BUILD_IDS.rehearsalPassed,
    {
      build_id: BUILD_IDS.rehearsalPassed,
      stores: ['google_play'],
      checks: [
        {
          store: 'google_play',
          code: 'play_version_code',
          level: 'ok',
          title: 'Version code 147 is free on Google Play',
          detail: 'The highest on production is 141.',
          remembered: false,
        },
        {
          store: 'google_play',
          code: 'play_validate',
          level: 'ok',
          title: 'Google Play accepts 2.5.2 (147) on production',
          detail:
            'Staged and checked by Google Play, then discarded. Nothing was committed.',
          remembered: false,
        },
      ],
    },
  ],
])
