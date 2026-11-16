import { PROJECT_IDS, ago } from '../seed'
import type { VersionReleaseNotes } from '@oore/client/models'

/**
 * One set of notes per app version, as the daemon keeps them: written in
 * Oore, drafted from commits, or read from a CHANGELOG.md in the repository.
 * Internal Admin has none, so its builds show the empty state.
 */
export const demoReleaseNotes: Array<VersionReleaseNotes> = [
  {
    project_id: PROJECT_IDS.flutterShop,
    version_name: '2.5.1',
    text: '• The checkout total is right on iPad again\n• Faster loading of your order history\n• Saved delivery addresses fill in correctly',
    locales: [
      {
        locale: 'de-DE',
        text: '• Der Gesamtbetrag stimmt auf dem iPad wieder\n• Ihre Bestellungen laden schneller',
      },
    ],
    source: 'manual',
    updated_at: ago(1500),
    updated_by: 'demo+owner@oore.build',
  },
  {
    project_id: PROJECT_IDS.flutterShop,
    version_name: '2.5.2',
    text: '• Order tracking map\n• Order tracking on the home screen\n• Smaller app size',
    locales: [],
    source: 'commits',
    drafted_since: '2.5.1',
    updated_at: ago(3000),
  },
  {
    project_id: PROJECT_IDS.flutterShop,
    version_name: '2.5.0',
    text: '• Track your order on the home screen\n• A smaller download\n• Fixes a crash when the cart is empty',
    locales: [],
    source: 'commits',
    drafted_since: '2.4.0',
    updated_at: ago(4000),
  },
  {
    project_id: PROJECT_IDS.flutterShop,
    version_name: '2.4.0',
    text: '• Wishlists you can share\n• Smoother cart updates',
    locales: [],
    source: 'manual',
    updated_at: ago(9000),
    updated_by: 'demo+developer@oore.build',
  },
  {
    project_id: PROJECT_IDS.nativePayments,
    version_name: '3.3.0',
    text: '### Added\n- Share receipts from the order screen\n\n### Fixed\n- Clearer messages when a payment needs a retry',
    locales: [],
    source: 'repository',
    source_path: 'CHANGELOG.md',
    updated_at: ago(1500),
  },
  {
    project_id: PROJECT_IDS.nativePayments,
    version_name: '3.2.0',
    text: '- Pay with saved cards in one tap\n- Fixes a crash when a refund is pending',
    locales: [],
    source: 'repository',
    source_path: 'CHANGELOG.md',
    updated_at: ago(9000),
  },
]

/** Notes a "Draft from commits" returns, by project. */
export function demoDraftNotes(
  commitTitles: Array<string>,
  sinceVersion: string | null,
) {
  const text = [...new Set(commitTitles)]
    .slice(0, 12)
    .map((title) => `• ${title}`)
    .join('\n')
  return { text, since_version: sinceVersion }
}
