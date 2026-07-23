import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ExternalAccessManagement } from './preferences-external-access-management'
import { RuntimeOverview } from './preferences-runtime-overview'
import type { PreferencesPageState } from '@/routes/settings/preferences'

function stateWith(
  values: Partial<PreferencesPageState>,
): PreferencesPageState {
  return values as PreferencesPageState
}

describe('Preferences deferred surfaces', () => {
  it('keeps a supervised backend update failure visible with its log path', () => {
    render(
      <RuntimeOverview
        state={stateWith({
          backendHealthQuery: { data: { channel: 'alpha' } } as never,
          backendUpdatePhase: 'failed',
          backendVersionLabel: '1.2.3-alpha.1',
          frontendUpdatePhase: 'idle',
          isOwner: true,
          runtimeUpdates: {
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
          } as never,
          webHealthQuery: { data: { channel: 'alpha' } } as never,
          webVersionLabel: '1.2.3-alpha.1',
        })}
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

  it('preloads only the dialog matching the focused control', () => {
    const preloadNetwork = vi.fn()
    const preloadOidc = vi.fn()
    const preloadTrustedProxy = vi.fn()

    const { rerender } = render(
      <ExternalAccessManagement
        state={stateWith({
          isOwner: true,
          networkSettings: undefined,
          networkSettingsQuery: { isLoading: false, error: null } as never,
          oidcConfigQuery: { isLoading: false, error: null } as never,
          preloadExternalAccessNetworkDialog: preloadNetwork,
          preloadOidcSettingsDialog: preloadOidc,
          preloadTrustedProxySettingsDialog: preloadTrustedProxy,
          remoteAuthMode: 'trusted_proxy',
          setNetworkEditorOpen: vi.fn(),
          setOidcDialogOpen: vi.fn(),
          setTrustedProxyDialogOpen: vi.fn(),
          trustedProxySettings: undefined,
          trustedProxyQuery: { isLoading: false, error: null } as never,
        })}
      />,
    )

    fireEvent.mouseEnter(
      screen.getByRole('button', { name: /Network settings/i }),
    )
    fireEvent.focus(screen.getByRole('button', { name: /Identity settings/i }))

    expect(preloadNetwork).toHaveBeenCalledOnce()
    expect(preloadTrustedProxy).toHaveBeenCalledOnce()
    expect(preloadOidc).not.toHaveBeenCalled()

    rerender(
      <ExternalAccessManagement
        state={stateWith({
          isOwner: true,
          networkSettings: undefined,
          networkSettingsQuery: { isLoading: false, error: null } as never,
          oidcConfigQuery: { isLoading: false, error: null } as never,
          preloadExternalAccessNetworkDialog: preloadNetwork,
          preloadOidcSettingsDialog: preloadOidc,
          preloadTrustedProxySettingsDialog: preloadTrustedProxy,
          remoteAuthMode: 'oidc',
          setNetworkEditorOpen: vi.fn(),
          setOidcDialogOpen: vi.fn(),
          setTrustedProxyDialogOpen: vi.fn(),
          trustedProxySettings: undefined,
          trustedProxyQuery: { isLoading: false, error: null } as never,
        })}
      />,
    )
    fireEvent.focus(screen.getByRole('button', { name: /Identity settings/i }))

    expect(preloadOidc).toHaveBeenCalledOnce()
  })
})
