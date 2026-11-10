import { useEffect, useRef } from 'react'
import { useNavigate } from '@tanstack/react-router'
import type { InboxItem } from '@oore/client/models'

import { useNotificationPreferences } from '@/hooks/use-personal'
import { itemsToAnnounce } from '@/lib/inbox'

/**
 * Shows new inbox items as browser notifications while Oore is open in a
 * background tab, as the person chose in their notification settings.
 */
export function useInboxAnnouncer(items: ReadonlyArray<InboxItem> | undefined) {
  const preferencesQuery = useNotificationPreferences()
  const announced = useRef<Set<string> | null>(null)
  const navigate = useNavigate()
  const preferences = preferencesQuery.data?.preferences

  useEffect(() => {
    if (!items || !preferences) return
    const fresh = itemsToAnnounce(items, announced.current, preferences)
    const known = announced.current ?? new Set<string>()
    for (const item of items) known.add(item.id)
    announced.current = known
    if (
      fresh.length === 0 ||
      globalThis.Notification === undefined ||
      Notification.permission !== 'granted' ||
      (!document.hidden && document.hasFocus())
    ) {
      return
    }
    for (const item of fresh.slice(0, 3)) {
      const notification = new Notification(item.title, {
        body: [item.project_name, item.detail].filter(Boolean).join(' · '),
        tag: item.id,
      })
      notification.onclick = () => {
        window.focus()
        if (item.kind === 'release' && item.release_id) {
          void navigate({
            to: '/releases/$releaseId',
            params: { releaseId: item.release_id },
          })
        } else if (item.build_id) {
          void navigate({
            to: '/builds/$buildId',
            params: { buildId: item.build_id },
          })
        }
      }
    }
  }, [items, preferences, navigate])
}
