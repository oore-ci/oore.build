import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { DirectRunnerPolicyPanel } from './direct-runner-policy-panel'

const mocks = vi.hoisted(() => ({
  permissions: {
    read: true,
    write: true,
  },
  preferencesQuery: {
    data: {
      preferences: {
        key_storage_mode: 'database',
        direct_macos_runner_enabled: false,
      },
    },
    error: null as Error | null,
    isLoading: false,
    refetch: vi.fn(),
  },
  updatePreferences: {
    isPending: false,
    mutate: vi.fn(),
  },
  useInstancePreferences: vi.fn(),
  useUpdateInstancePreferences: vi.fn(),
}))

vi.mock('@/hooks/use-artifact-storage', () => ({
  useInstancePreferences: mocks.useInstancePreferences,
  useUpdateInstancePreferences: mocks.useUpdateInstancePreferences,
}))

vi.mock('@/hooks/use-permissions', () => ({
  useHasPermission: (_resource: string, action: 'read' | 'write') =>
    mocks.permissions[action],
}))

describe('DirectRunnerPolicyPanel', () => {
  beforeEach(() => {
    mocks.permissions.read = true
    mocks.permissions.write = true
    mocks.preferencesQuery.data = {
      preferences: {
        key_storage_mode: 'database',
        direct_macos_runner_enabled: false,
      },
    }
    mocks.preferencesQuery.error = null
    mocks.preferencesQuery.isLoading = false
    mocks.preferencesQuery.refetch.mockReset()
    mocks.updatePreferences.isPending = false
    mocks.updatePreferences.mutate.mockReset()
    mocks.useInstancePreferences.mockReset()
    mocks.useInstancePreferences.mockReturnValue(mocks.preferencesQuery)
    mocks.useUpdateInstancePreferences.mockReset()
    mocks.useUpdateInstancePreferences.mockReturnValue(mocks.updatePreferences)
  })

  it('omits the admin policy and preferences queries without instance settings access', () => {
    mocks.permissions.read = false
    mocks.permissions.write = false

    render(<DirectRunnerPolicyPanel />)

    expect(screen.queryByLabelText('Direct runner policy')).toBeNull()
    expect(mocks.useInstancePreferences).not.toHaveBeenCalled()
    expect(mocks.useUpdateInstancePreferences).not.toHaveBeenCalled()
  })

  it('updates the instance-wide policy from the labeled Runners control', () => {
    render(<DirectRunnerPolicyPanel />)

    fireEvent.click(
      screen.getByRole('switch', { name: 'Allow approved repositories' }),
    )

    expect(mocks.updatePreferences.mutate).toHaveBeenCalledWith(
      {
        key_storage_mode: 'database',
        direct_macos_runner_enabled: true,
      },
      expect.any(Object),
    )
  })

  it('shows a retry path instead of a false disabled state when loading fails', () => {
    mocks.preferencesQuery.data = undefined as never
    mocks.preferencesQuery.error = new Error('service unavailable')

    render(<DirectRunnerPolicyPanel />)

    expect(screen.queryByRole('switch')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }))
    expect(mocks.preferencesQuery.refetch).toHaveBeenCalledOnce()
  })
})
