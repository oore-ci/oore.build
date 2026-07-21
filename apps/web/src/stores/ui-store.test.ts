import { beforeEach, describe, expect, it } from 'vitest'

import { useUiStore } from './ui-store'

beforeEach(() => {
  localStorage.clear()
  useUiStore.setState({
    commandPaletteOpen: false,
    sidebarOpen: true,
  })
})

describe('useUiStore', () => {
  it('persists the sidebar preference without transient UI state', () => {
    useUiStore.getState().setSidebarOpen(false)
    useUiStore.getState().setCommandPaletteOpen(true)

    const stored = JSON.parse(
      localStorage.getItem('oore-ui-preferences') ?? '{}',
    ) as { state?: Record<string, unknown> }

    expect(stored.state).toEqual({
      sidebarOpen: false,
    })
  })

  it('restores the sidebar preference from a previous session', async () => {
    localStorage.setItem(
      'oore-ui-preferences',
      JSON.stringify({ state: { sidebarOpen: false }, version: 0 }),
    )

    await useUiStore.persist.rehydrate()

    expect(useUiStore.getState().sidebarOpen).toBe(false)
  })
})
