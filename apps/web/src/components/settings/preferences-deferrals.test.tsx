import { fireEvent, render, screen } from '@testing-library/react'
import { useForm } from 'react-hook-form'
import { describe, expect, it, vi } from 'vitest'

import { ArtifactStorageSettings } from './preferences-artifact-storage-settings'
import { ExternalAccessManagement } from './preferences-external-access-management'
import type { ArtifactStoragePageState } from './use-artifact-storage-page-state'
import type { PreferencesPageState } from '@/routes/settings/preferences'

function stateWith(
  values: Partial<PreferencesPageState>,
): PreferencesPageState {
  return values as PreferencesPageState
}

function artifactStateWith(
  values: Partial<ArtifactStoragePageState>,
): ArtifactStoragePageState {
  return values as ArtifactStoragePageState
}

describe('Preferences deferred surfaces', () => {
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

  it('preloads the local folder picker before opening it', () => {
    const preloadFolderPicker = vi.fn()
    const setFolderPickerOpen = vi.fn()

    function Harness() {
      const storageForm = useForm({
        defaultValues: {
          backend_kind: 'local',
          local_base_dir: '/tmp/artifacts',
        },
      })
      return (
        <ArtifactStorageSettings
          state={artifactStateWith({
            backendKind: 'local',
            canBrowseLocalFs: true,
            canWrite: true,
            onSubmitStorage: vi.fn(),
            preloadArtifactFolderPicker: preloadFolderPicker,
            setArtifactDirPickerOpen: setFolderPickerOpen,
            settings: {} as never,
            settingsQuery: { isLoading: false, error: null } as never,
            storageForm: storageForm as never,
            updateStorageMutation: { isPending: false } as never,
          })}
        />
      )
    }

    render(<Harness />)
    const trigger = screen.getByRole('button', {
      name: 'Browse local base directory',
    })
    fireEvent.mouseEnter(trigger)
    fireEvent.click(trigger)

    expect(preloadFolderPicker).toHaveBeenCalledOnce()
    expect(setFolderPickerOpen).toHaveBeenCalledWith(true)
  })

  it('does not render editable artifact defaults when settings are missing', () => {
    render(
      <ArtifactStorageSettings
        state={artifactStateWith({
          settings: undefined,
          settingsQuery: {
            error: null,
            isLoading: false,
            refetch: vi.fn(),
          } as never,
        })}
      />,
    )

    expect(
      screen.getByText(/did not include artifact storage settings/i),
    ).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Save' })).toBeNull()
  })
})
