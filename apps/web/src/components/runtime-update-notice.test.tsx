import { fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import RuntimeUpdateNotice from './runtime-update-notice'
import { formatReleaseNotes } from './runtime-update-utils'
import { SidebarProvider } from '@/components/ui/sidebar'

const { useRuntimeUpdates } = vi.hoisted(() => ({
  useRuntimeUpdates: vi.fn(),
}))

vi.mock('@/hooks/use-runtime-updates', () => ({ useRuntimeUpdates }))

const release = {
  phase: 'idle' as const,
  error: null,
  managed_service: true,
  version: '1.2.3-alpha.1',
  latest_version: '1.2.3-alpha.2',
  channel: 'alpha',
  github_repo: 'example/oore',
  update_available: true,
  release_name: 'Alpha 2',
  release_notes: '- fix(web): show updates everywhere',
  release_url: 'https://github.com/example/oore/releases/tag/v1.2.3-alpha.2',
  changelog_url:
    'https://github.com/example/oore/compare/v1.2.3-alpha.1...v1.2.3-alpha.2',
}

beforeEach(() => {
  window.matchMedia = vi.fn().mockReturnValue({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })
  useRuntimeUpdates.mockReturnValue({
    frontendRelease: { data: release },
    backendRelease: { data: release },
    backendUpdate: { data: { managed_service: true, phase: 'idle' } },
    startFrontendUpdate: { data: undefined, isPending: false, mutate: vi.fn() },
    startBackendUpdate: { data: undefined, isPending: false, mutate: vi.fn() },
  })
})

describe('formatReleaseNotes', () => {
  it('keeps the useful release summary without duplicate headings and links', () => {
    expect(
      formatReleaseNotes(`# v1.2.3

Changes since v1.2.2:

- fix(web): show updates everywhere

**Full Changelog**: https://github.com/example/oore/compare/v1.2.2...v1.2.3`),
    ).toBe('Changes since v1.2.2:\n\n- fix(web): show updates everywhere')
  })
})

describe('RuntimeUpdateNotice', () => {
  it('opens a dialog with both runtimes and one shared changelog', async () => {
    render(
      <SidebarProvider>
        <RuntimeUpdateNotice />
      </SidebarProvider>,
    )

    fireEvent.click(screen.getByRole('button', { name: /Updates available/i }))

    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('Frontend')).toBeTruthy()
    expect(within(dialog).getByText('Backend')).toBeTruthy()
    expect(within(dialog).getAllByText('1.2.3-alpha.1')).toHaveLength(2)
    expect(within(dialog).getAllByText('1.2.3-alpha.2')).toHaveLength(2)
    expect(within(dialog).getAllByText(/show updates everywhere/)).toHaveLength(
      1,
    )
    expect(
      within(dialog)
        .getByRole('link', { name: /Full changelog/i })
        .getAttribute('href'),
    ).toBe(release.changelog_url)
  })

  it('requires the one-time installer repair for a wrapper runner service', async () => {
    useRuntimeUpdates.mockReturnValue({
      frontendRelease: { data: release },
      backendRelease: { data: release },
      backendUpdate: { data: { managed_service: false, phase: 'idle' } },
      startFrontendUpdate: {
        data: undefined,
        isPending: false,
        mutate: vi.fn(),
      },
      startBackendUpdate: {
        data: undefined,
        isPending: false,
        mutate: vi.fn(),
      },
    })

    render(
      <SidebarProvider>
        <RuntimeUpdateNotice />
      </SidebarProvider>,
    )

    fireEvent.click(screen.getByRole('button', { name: /Updates available/i }))

    const dialog = await screen.findByRole('dialog')
    expect(
      within(dialog).getByText(
        /Run the current installer once from Terminal to finish or repair managed service setup/i,
      ),
    ).toBeTruthy()
    expect(
      within(dialog).getByText(
        'curl -fsSL https://alpha.oore.pages.dev/install | OORE_CHANNEL=alpha bash',
      ),
    ).toBeTruthy()
    expect(
      within(dialog)
        .getAllByRole('button', { name: 'Update now' })[1]
        .hasAttribute('disabled'),
    ).toBe(true)
  })
})
