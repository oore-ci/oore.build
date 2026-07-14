import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SidebarInset, SidebarMenuButton, SidebarProvider } from './sidebar'

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

describe('SidebarInset', () => {
  it('shrinks beside the sidebar instead of widening the document', () => {
    render(<SidebarInset>Content</SidebarInset>)

    const inset = screen.getByRole('main')
    expect(inset.className).toContain('min-w-0')
    expect(inset.className).not.toContain('w-full')
  })
})
