import { useAppShortcut } from '@/hooks/use-app-shortcut'
import { SHORTCUTS, stepListIndex } from '@/lib/keyboard-shortcuts'

function visibleItems() {
  const root = document.getElementById('main-content') ?? document
  return Array.from(
    root.querySelectorAll<HTMLElement>('[data-list-nav-item]'),
  ).filter((item) => item.getClientRects().length > 0)
}

function moveFocus(direction: 1 | -1) {
  const items = visibleItems()
  const active = document.activeElement
  const current = items.findIndex(
    (item) => item === active || item.contains(active),
  )
  const next = items[stepListIndex(current, items.length, direction)]
  if (!next) return
  next.focus()
  next.scrollIntoView({ block: 'nearest' })
}

/**
 * `j` / `k` move focus through the page's rows and `Enter` opens the focused
 * one. Rows opt in with `LIST_NAV_ITEM` on their primary link.
 */
export function useListNavigation(enabled = true) {
  useAppShortcut(SHORTCUTS.listNext, () => moveFocus(1), enabled)
  useAppShortcut(SHORTCUTS.listPrevious, () => moveFocus(-1), enabled)
  useAppShortcut(
    SHORTCUTS.listOpen,
    () => {
      const active = document.activeElement
      if (
        !(active instanceof HTMLElement) ||
        !active.matches('[data-list-nav-item]')
      ) {
        return false
      }
      active.click()
    },
    enabled,
  )
}
