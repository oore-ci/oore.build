import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useIsMobile } from './use-mobile'

function BreakpointProbe() {
  return <span>{useIsMobile() ? 'compact' : 'desktop'}</span>
}

describe('useIsMobile', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'matchMedia',
      vi.fn().mockImplementation((query: string) => ({
        matches: window.innerWidth <= Number(query.match(/\d+/)?.[0] ?? 0),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    )
  })

  it('keeps the sidebar compact at tablet width', () => {
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      value: 768,
    })

    render(<BreakpointProbe />)

    expect(screen.getByText('compact')).toBeTruthy()
  })

  it('switches to the desktop sidebar at 1024px', () => {
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      value: 1024,
    })

    render(<BreakpointProbe />)

    expect(screen.getByText('desktop')).toBeTruthy()
  })
})
