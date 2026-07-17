import { useState } from 'react'
import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { IntegrationRepository } from '@/lib/types'
import { GitLabWebhookTokenDialogs } from './-gitlab-webhook-tokens'

const mocks = vi.hoisted(() => ({ isPending: false, mutate: vi.fn() }))

vi.mock('@/hooks/use-integrations', () => ({
  useRotateGitLabRepositoryWebhookSecret: () => ({
    isPending: mocks.isPending,
    mutate: mocks.mutate,
  }),
}))

const repository: IntegrationRepository = {
  id: 'repository-1',
  installation_id: 'installation-1',
  external_id: '1',
  full_name: 'group/mobile-app',
  default_branch: 'main',
  is_private: true,
  allow_direct_macos_runner: false,
  created_at: 1,
  updated_at: 1,
}

function Harness() {
  const [target, setTarget] = useState<IntegrationRepository | null>(repository)
  return (
    <GitLabWebhookTokenDialogs
      repository={target}
      webhookUrl="https://ci.example.test/v1/webhooks/gitlab"
      onClose={() => setTarget(null)}
    />
  )
}

describe('GitLabWebhookTokenDialogs', () => {
  beforeEach(() => {
    mocks.isPending = false
    mocks.mutate.mockReset()
  })

  it('confirms rotation, identifies the project, and clears the revealed token', async () => {
    render(<Harness />)

    expect(mocks.mutate).not.toHaveBeenCalled()
    const confirmation = await screen.findByRole('alertdialog')
    expect(within(confirmation).getByText(/group\/mobile-app/)).toBeTruthy()

    fireEvent.click(
      within(confirmation).getByRole('button', { name: 'Create token' }),
    )
    expect(mocks.mutate).toHaveBeenCalledTimes(1)
    expect(mocks.mutate.mock.calls[0]?.[0]).toBe(repository.id)
    expect(screen.getByRole('alertdialog')).toBeTruthy()

    act(() => {
      mocks.mutate.mock.calls[0]?.[1].onSuccess({
        webhook_secret: 'one-time-secret',
      })
    })

    const dialog = await screen.findByRole('dialog', {
      name: 'Webhook token for group/mobile-app',
    })
    expect(within(dialog).getByDisplayValue('one-time-secret')).toBeTruthy()
    expect(
      within(dialog).getByDisplayValue(
        'https://ci.example.test/v1/webhooks/gitlab',
      ),
    ).toBeTruthy()

    fireEvent.click(within(dialog).getByRole('button', { name: 'Done' }))
    expect(screen.queryByDisplayValue('one-time-secret')).toBeNull()
  })

  it('cannot be dismissed while a token rotation is pending', async () => {
    mocks.isPending = true
    render(<Harness />)

    const confirmation = await screen.findByRole('alertdialog')
    const cancel = within(confirmation).getByRole('button', { name: 'Cancel' })
    expect(cancel.hasAttribute('disabled')).toBe(true)

    fireEvent.keyDown(document, { key: 'Escape' })
    expect(screen.getByRole('alertdialog')).toBeTruthy()
  })
})
