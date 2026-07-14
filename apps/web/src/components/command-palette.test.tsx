import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import CommandPalette from './command-palette'
import { useUiStore } from '@/stores/ui-store'

const { navigate } = vi.hoisted(() => ({ navigate: vi.fn() }))

vi.stubGlobal(
  'ResizeObserver',
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
)

Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
  configurable: true,
  value: vi.fn(),
})

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigate,
}))

vi.mock('@/hooks/use-projects', () => ({
  useProjects: () => ({ data: { projects: [] } }),
}))

vi.mock('@/hooks/use-permissions', () => ({
  useHasPermission: () => true,
}))

vi.mock('@/stores/auth-store', () => ({
  useAuthStore: (selector: (state: unknown) => unknown) =>
    selector({ user: { role: 'owner' } }),
}))

describe('CommandPalette', () => {
  beforeEach(() => {
    navigate.mockReset()
    useUiStore.setState({ commandPaletteOpen: false })
  })

  it('opens with Cmd+K and renders searchable commands', async () => {
    render(<CommandPalette />)

    fireEvent.keyDown(window, { key: 'k', metaKey: true })

    expect(await screen.findByRole('dialog')).toBeTruthy()
    expect(
      screen.getByPlaceholderText('Search projects, pages, actions...'),
    ).toBeTruthy()
    expect(screen.getByText('Dashboard')).toBeTruthy()
  })
})
