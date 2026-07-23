import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SidebarMenuButton, SidebarProvider } from './sidebar'

vi.stubGlobal(
  'matchMedia',
  vi.fn().mockReturnValue({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }),
)

describe('SidebarMenuButton', () => {
  it('preserves a rendered link when a tooltip is enabled', () => {
    render(
      <SidebarProvider defaultOpen={false}>
        <SidebarMenuButton tooltip="Builds" render={<a href="/builds" />}>
          Builds
        </SidebarMenuButton>
      </SidebarProvider>,
    )

    expect(
      screen.getByRole('link', { name: 'Builds' }).getAttribute('href'),
    ).toBe('/builds')
  })
})
