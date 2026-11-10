import type { InboxItem, NotificationPreferences } from '@oore/client/models'

/** Items newer than the last time the bell was opened. */
export function unreadCount(
  items: ReadonlyArray<Pick<InboxItem, 'at'>>,
  seenAt: number,
): number {
  return items.filter((item) => item.at > seenAt).length
}

/** Whether a person's choices let this inbox item reach them. */
export function wantsInboxItem(
  preferences: Pick<
    NotificationPreferences,
    'approvals' | 'followed_releases' | 'my_builds'
  >,
  item: Pick<InboxItem, 'kind'>,
): boolean {
  switch (item.kind) {
    case 'approval':
      return preferences.approvals
    case 'failed_build':
      return preferences.my_builds !== 'off'
    case 'release':
      return preferences.followed_releases
    default:
      return false
  }
}

/**
 * Items to show as browser notifications: new since the last check, wanted
 * by the person, and not already shown. The first load only records what
 * exists, so opening Oore does not replay the week.
 */
export function itemsToAnnounce(
  items: ReadonlyArray<InboxItem>,
  announced: ReadonlySet<string> | null,
  preferences: Pick<
    NotificationPreferences,
    'approvals' | 'browser' | 'followed_releases' | 'my_builds'
  >,
): Array<InboxItem> {
  if (announced === null || !preferences.browser) return []
  return items.filter(
    (item) => !announced.has(item.id) && wantsInboxItem(preferences, item),
  )
}
