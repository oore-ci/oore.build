import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { RuntimeOverview } from './preferences-runtime-overview'

describe('Preferences deferred surfaces', () => {
  it('keeps a supervised backend update failure visible with its log path', () => {
    render(
      <RuntimeOverview
        backendUpdatePhase="failed"
        backendVersionLabel="1.2.3-alpha.1"
        frontendUpdatePhase="idle"
        isOwner
        runtimeUpdates={
          {
            backendHealth: { data: { channel: 'alpha' } },
            backendRelease: {
              data: {
                latest_version: '1.2.3-alpha.2',
                update_available: true,
              },
            },
            backendUpdate: {
              data: {
                error: 'Candidate readiness check failed; rollback completed.',
                managed_service: true,
                phase: 'failed',
              },
            },
            frontendRelease: { data: undefined },
            startBackendUpdate: {
              isPending: false,
              mutate: vi.fn(),
            },
            frontendHealth: { data: { channel: 'alpha' } },
          } as never
        }
        webVersionLabel="1.2.3-alpha.1"
      />,
    )

    expect(screen.getByText('Backend update failed')).toBeTruthy()
    expect(
      screen.getByText('Candidate readiness check failed; rollback completed.'),
    ).toBeTruthy()
    expect(
      screen.getByText('<install root>/logs/update-supervisor.log'),
    ).toBeTruthy()
    expect(
      screen.getByRole('button', { name: 'Retry backend update' }),
    ).toBeTruthy()
  })
})
