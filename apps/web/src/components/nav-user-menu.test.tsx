import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import NavUserMenu from './nav-user-menu'
import { SidebarProvider } from './ui/sidebar'
import { useAuthStore } from '@/stores/auth-store'

vi.stubGlobal(
  'matchMedia',
  vi.fn().mockReturnValue({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }),
)

vi.mock('@/lib/demo-mode', () => ({ isDemoMode: true }))
vi.mock('@/hooks/use-auth', () => ({
  useLogout: () => ({ mutate: vi.fn(), isPending: false }),
}))
vi.mock('next-themes', () => ({
  useTheme: () => ({ theme: 'system', setTheme: vi.fn() }),
}))

describe('NavUserMenu demo tools', () => {
  beforeEach(() => {
    useAuthStore.setState({
      instanceId: 'demo-instance',
      token: 'demo-session-token-owner',
      expiresAt: 9_999_999_999,
      user: {
        email: 'demo+owner@oore.build',
        oidc_subject: 'demo::owner',
        user_id: 'owner',
        role: 'owner',
      },
    })
  })

  afterEach(() => {
    useAuthStore.setState({ token: null, expiresAt: null, user: null })
  })

  it('exposes concise, accessible persona and scenario controls', async () => {
    render(
      <SidebarProvider>
        <NavUserMenu open onOpenChange={vi.fn()} />
      </SidebarProvider>,
    )

    expect(screen.getByText('Demo tools')).toBeTruthy()
    const persona = screen.getByRole('menuitem', {
      name: 'Persona: Owner',
    })
    expect(
      screen.getByRole('menuitem', { name: 'Scenario: Operating' }),
    ).toBeTruthy()

    fireEvent.click(persona)
    expect(
      await screen.findByRole('menuitemradio', { name: 'Developer' }),
    ).toBeTruthy()
  })
})
