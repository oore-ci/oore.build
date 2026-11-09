import type { Release } from '@oore/client/models'

/** A store refusal in one sentence, with the page that fixes it. */
export interface PlainRefusal {
  code: string
  /** What is wrong, in one sentence. */
  sentence: string
  /** The button label for the fix. */
  fix: string
  href: string | null
}

type RefusalContext = Pick<Release, 'store' | 'console_url'>

/** The App Store Connect app id in a release's console link. */
function appStoreAppId(consoleUrl: string | null | undefined) {
  return consoleUrl?.match(/appstoreconnect\.apple\.com\/apps\/(\d+)/)?.[1]
}

function appStorePage(release: RefusalContext, page: string) {
  const appId = appStoreAppId(release.console_url)
  return appId
    ? `https://appstoreconnect.apple.com/apps/${appId}/${page}`
    : (release.console_url ?? null)
}

interface Rule {
  code: string
  store?: Release['store']
  matches: (text: string) => boolean
  explain: (release: RefusalContext) => Omit<PlainRefusal, 'code'>
}

const has =
  (...needles: Array<string>) =>
  (text: string) =>
    needles.some((needle) => text.includes(needle))

const VERSION_PAGE = 'distribution/ios/version/inflight'

/**
 * The refusals people meet most, first match wins. Apple and Google word
 * them differently over time, so each matches a few phrasings.
 */
const RULES: Array<Rule> = [
  {
    code: 'changes_not_sent_for_review',
    store: 'google_play',
    matches: has(
      'changesnotsentforreview',
      'changes not sent for review',
      'send for review',
      'sent for review',
    ),
    explain: (release) => ({
      sentence:
        'Google Play is holding the changes until someone sends them for review in Play Console.',
      fix: 'Send changes for review',
      href: release.console_url ?? null,
    }),
  },
  {
    code: 'permission',
    store: 'google_play',
    matches: has('permission', 'not authorized', 'unauthorized', 'forbidden'),
    explain: () => ({
      sentence:
        "Oore's service account is not allowed to release this app. Give it release access in Play Console under Users and permissions.",
      fix: 'Open Users and permissions',
      href: 'https://play.google.com/console/users-and-permissions',
    }),
  },
  {
    code: 'whats_new',
    matches: has(
      "what's new",
      'whatsnew',
      'whats new',
      'release notes',
      'releasenotes',
    ),
    explain: (release) => ({
      sentence:
        release.store === 'app_store'
          ? 'This version has no "What\'s New" text. Add it in App Store Connect, or write notes when you start the next build.'
          : 'The release has no release notes. Add them in Play Console, or write notes when you start the next build.',
      fix: 'Add release notes',
      href:
        release.store === 'app_store'
          ? appStorePage(release, VERSION_PAGE)
          : (release.console_url ?? null),
    }),
  },
  {
    code: 'age_rating',
    matches: has('age rating', 'agerating', 'content rating'),
    explain: (release) => ({
      sentence:
        release.store === 'app_store'
          ? 'Apple needs the age rating questions answered before review.'
          : 'Google Play needs the content rating questionnaire filled in.',
      fix: 'Answer the rating questions',
      href:
        release.store === 'app_store'
          ? appStorePage(release, 'distribution/info')
          : (release.console_url ?? null),
    }),
  },
  {
    code: 'review_details',
    store: 'app_store',
    matches: has('review detail', 'contact', 'demo account', 'sign-in'),
    explain: (release) => ({
      sentence:
        'App Review needs a contact name, phone number and email, and a demo account if the app has sign-in.',
      fix: 'Add App Review details',
      href: appStorePage(release, VERSION_PAGE),
    }),
  },
  {
    code: 'export_compliance',
    store: 'app_store',
    matches: has('export compliance', 'encryption'),
    explain: (release) => ({
      sentence:
        'Apple needs to know whether the build uses encryption before testers or reviewers can have it.',
      fix: 'Answer export compliance',
      href: appStorePage(release, 'testflight/ios'),
    }),
  },
  {
    code: 'privacy',
    matches: has('privacy', 'data collection', 'data safety'),
    explain: (release) => ({
      sentence:
        release.store === 'app_store'
          ? 'The App Privacy answers are incomplete.'
          : 'The Data safety form is incomplete.',
      fix:
        release.store === 'app_store'
          ? 'Finish App Privacy'
          : 'Finish Data safety',
      href:
        release.store === 'app_store'
          ? appStorePage(release, 'distribution/privacy')
          : (release.console_url ?? null),
    }),
  },
  {
    code: 'agreement',
    matches: has('agreement', 'contract', 'tax', 'banking'),
    explain: (release) => ({
      sentence:
        'An agreement, tax or banking form is waiting for the account holder.',
      fix: 'Open agreements',
      href:
        release.store === 'app_store'
          ? 'https://appstoreconnect.apple.com/business'
          : (release.console_url ?? null),
    }),
  },
  {
    code: 'screenshots',
    matches: has('screenshot'),
    explain: (release) => ({
      sentence: 'The store listing is missing screenshots for a device size.',
      fix: 'Add screenshots',
      href:
        release.store === 'app_store'
          ? appStorePage(release, VERSION_PAGE)
          : (release.console_url ?? null),
    }),
  },
  {
    code: 'version_used',
    matches: (text) =>
      text.includes('already') &&
      (text.includes('version') || text.includes('build')),
    explain: () => ({
      sentence:
        'That version or build number was uploaded before. Raise the build number in pubspec.yaml and build again.',
      fix: '',
      href: null,
    }),
  },
]

/**
 * Each problem a store's refusal names, in plain words with one link that
 * fixes it. Empty when Oore recognizes none and the store's own text has to
 * do. Apple often lists several problems in one refusal.
 */
export function plainRefusals(
  message: string | null | undefined,
  release: RefusalContext,
): Array<PlainRefusal> {
  const text = message?.toLowerCase().trim()
  if (!text) return []
  return RULES.filter(
    (rule) =>
      (!rule.store || rule.store === release.store) && rule.matches(text),
  ).map((rule) => ({ code: rule.code, ...rule.explain(release) }))
}

/** The first problem a refusal names, or `null`. */
export function plainRefusal(
  message: string | null | undefined,
  release: RefusalContext,
): PlainRefusal | null {
  return plainRefusals(message, release)[0] ?? null
}

/** "Guideline 2.1" and friends named in an App Review rejection. */
export function reviewGuidelines(message: string | null | undefined) {
  if (!message) return []
  const found = message.matchAll(/guideline\s+(\d+(?:\.\d+)*)/gi)
  return [...new Set([...found].map((match) => match[1]))]
}
