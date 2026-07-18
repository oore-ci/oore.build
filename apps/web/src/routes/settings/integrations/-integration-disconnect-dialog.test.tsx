import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { Integration, Project } from '@/lib/types'
import { IntegrationDisconnectDialog } from './-integration-disconnect-dialog'

const integration: Integration = {
  id: 'integration-1',
  provider: 'gitlab',
  host_url: 'https://gitlab.example.test',
  auth_mode: 'oauth_app',
  status: 'active',
  display_name: 'Production GitLab',
  created_by: 'owner-1',
  created_at: 1,
  updated_at: 1,
}

function project(id: string, name: string): Project {
  return {
    id,
    name,
    repository_id: `repository-${id}`,
    settings: {},
    created_by: 'owner-1',
    created_at: 1,
    updated_at: 1,
  }
}

function renderDialog(
  overrides: Partial<
    React.ComponentProps<typeof IntegrationDisconnectDialog>
  > = {},
) {
  const props: React.ComponentProps<typeof IntegrationDisconnectDialog> = {
    affectedProjects: [],
    error: null,
    integration,
    isLoading: false,
    isPending: false,
    onConfirm: vi.fn(),
    onOpenChange: vi.fn(),
    onRetry: vi.fn(),
    open: true,
    repositoryCount: 34,
    ...overrides,
  }
  render(<IntegrationDisconnectDialog {...props} />)
  return props
}

describe('IntegrationDisconnectDialog', () => {
  it('shows affected projects and consequences before enabling disconnect', () => {
    const props = renderDialog({
      affectedProjects: [
        project('1', 'macOS release'),
        project('2', 'iOS nightly'),
      ],
    })

    expect(screen.getByText('macOS release')).toBeTruthy()
    expect(screen.getByText('iOS nightly')).toBeTruthy()
    expect(screen.getByText(/repository links are cleared/i)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Disconnect source' }))
    expect(props.onConfirm).toHaveBeenCalledOnce()
  })

  it('blocks disconnect and offers retry when the preview fails', () => {
    const props = renderDialog({ error: new Error('projects unavailable') })

    expect(
      screen
        .getByRole('button', { name: 'Disconnect source' })
        .hasAttribute('disabled'),
    ).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: 'Retry preview' }))
    expect(props.onRetry).toHaveBeenCalledOnce()
  })
})
