import { lazy, Suspense } from 'react'
import { useNavigate } from '@tanstack/react-router'

import { useAppSequence, useAppShortcut } from '@/hooks/use-app-shortcut'
import { SHORTCUTS } from '@/lib/keyboard-shortcuts'
import { useCommandStore } from '@/stores/command-store'

const KeyboardMapDialog = lazy(() => import('./keyboard-map-dialog'))

function focusPageSearch() {
  const root = document.getElementById('main-content') ?? document
  const search = Array.from(
    root.querySelectorAll<HTMLInputElement>('[data-page-search]'),
  ).find((input) => !input.disabled && input.getClientRects().length > 0)
  if (!search) return false
  search.focus()
  search.select()
  return true
}

/**
 * The operator workspace's app-wide shortcuts: the `g` jumps, `/` search and
 * `?` for the keyboard map. Page shortcuts register where the page renders.
 */
export function GlobalShortcuts() {
  const navigate = useNavigate()
  const keyboardMapOpen = useCommandStore((s) => s.keyboardMapOpen)
  const setKeyboardMapOpen = useCommandStore((s) => s.setKeyboardMapOpen)
  const setPaletteOpen = useCommandStore((s) => s.setPaletteOpen)

  useAppShortcut(SHORTCUTS.keyboardMap, () => setKeyboardMapOpen(true))
  // Without a search box on the page, `/` searches everything.
  useAppShortcut(SHORTCUTS.search, () => {
    if (!focusPageSearch()) setPaletteOpen(true)
  })
  useAppSequence(SHORTCUTS.goHome, () => void navigate({ to: '/' }))
  useAppSequence(SHORTCUTS.goBuilds, () => void navigate({ to: '/builds' }))
  useAppSequence(SHORTCUTS.goProjects, () => void navigate({ to: '/projects' }))
  useAppSequence(SHORTCUTS.goSettings, () => void navigate({ to: '/settings' }))

  return keyboardMapOpen ? (
    <Suspense fallback={null}>
      <KeyboardMapDialog open onOpenChange={setKeyboardMapOpen} />
    </Suspense>
  ) : null
}
