import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { LocalGitPage } from './local-git'

const state = vi.hoisted(() => ({
  role: 'developer',
  preferencesOptions: vi.fn(),
}))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (options: unknown) => options,
  Link: ({ children, to }: { children?: React.ReactNode; to: string }) => (
    <a href={to}>{children}</a>
  ),
}))

vi.mock('@/lib/instance-context', () => ({
  getActiveInstanceOrRedirect: vi.fn(),
  requireInstanceRoleOrRedirect: vi.fn(),
}))

vi.mock('@/hooks/use-permissions', () => ({
  useHasPermission: () => state.role === 'owner',
}))

vi.mock('@/hooks/use-artifact-storage', () => ({
  useInstancePreferences: (options: { enabled: boolean }) => {
    state.preferencesOptions(options)
    return {
      data: { preferences: { runtime_mode: 'remote' } },
    }
  },
}))

describe('LocalGitPage', () => {
  beforeEach(() => {
    state.role = 'developer'
    state.preferencesOptions.mockReset()
  })

  it('keeps the page read-only for developers without loading preferences', () => {
    render(<LocalGitPage />)

    expect(state.preferencesOptions).toHaveBeenCalledWith({ enabled: false })
    expect(screen.queryByRole('link', { name: 'Create project' })).toBeNull()
    expect(screen.queryByText(/In External Access mode/)).toBeNull()
    expect(
      screen.getByText(/Ask an owner or admin to add or change a source/),
    ).toBeTruthy()
  })

  it('keeps project creation and runtime guidance for owners', () => {
    state.role = 'owner'
    render(<LocalGitPage />)

    expect(state.preferencesOptions).toHaveBeenCalledWith({ enabled: true })
    expect(screen.getByRole('link', { name: 'Create project' })).toBeTruthy()
    expect(screen.getByText(/In External Access mode/)).toBeTruthy()
  })
})
