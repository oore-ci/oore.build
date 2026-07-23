import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import CommandPalette from './command-palette'

const { navigate, authState } = vi.hoisted(() => ({
  navigate: vi.fn(),
  authState: { role: 'owner' },
}))

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
    selector({ user: { role: authState.role } }),
}))

describe('CommandPalette', () => {
  beforeEach(() => {
    navigate.mockReset()
    authState.role = 'owner'
  })

  it('renders searchable commands when opened by the app shell', async () => {
    render(<CommandPalette open onOpenChange={vi.fn()} />)

    expect(await screen.findByRole('dialog')).toBeTruthy()
    expect(
      screen.getByPlaceholderText('Search projects, pages, actions...'),
    ).toBeTruthy()
    expect(screen.getByText('Dashboard')).toBeTruthy()
  })

  it('shows a build-only navigation surface for QA viewers', async () => {
    authState.role = 'qa_viewer'
    render(<CommandPalette open onOpenChange={vi.fn()} />)

    expect(await screen.findByRole('dialog')).toBeTruthy()
    expect(screen.getByText('Builds')).toBeTruthy()
    expect(screen.queryByText('Dashboard')).toBeNull()
    expect(screen.queryByText('Projects')).toBeNull()
  })
})
