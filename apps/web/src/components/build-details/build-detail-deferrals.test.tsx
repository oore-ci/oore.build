import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { ArtifactsPanel } from './artifacts-panel'
import ChangelogMarkdown from './changelog-markdown'
import { useAuthStore } from '@/stores/auth-store'

afterEach(() => {
  useAuthStore.setState({ user: null })
})

describe('build detail deferrals', () => {
  it('keeps the share trigger focused while preloading and opens its menu', async () => {
    useAuthStore.setState({
      user: {
        email: 'owner@example.com',
        oidc_subject: 'owner',
        user_id: 'owner',
        role: 'owner',
      },
    })
    const queryClient = new QueryClient()

    render(
      <QueryClientProvider client={queryClient}>
        <ArtifactsPanel
          artifacts={[
            {
              id: 'artifact-1',
              build_id: 'build-1',
              name: 'archive.zip',
              artifact_type: 'generic',
              file_path: 'archive.zip',
              metadata: {},
              created_at: 1,
            },
          ]}
          isLoading={false}
          buildStatus="succeeded"
        />
      </QueryClientProvider>,
    )

    const trigger = screen.getByRole('button', {
      name: 'Share options for archive.zip',
    })
    trigger.focus()
    await waitFor(() => expect(document.activeElement).toBe(trigger))

    fireEvent.click(trigger)

    expect(await screen.findByRole('menu')).toBeTruthy()
    expect(screen.getByText('Create share link')).toBeTruthy()
  })

  it('preserves safe changelog Markdown rendering', () => {
    render(
      <ChangelogMarkdown>
        {'- [Release notes](https://example.com)\n\n<strong>Safe</strong>'}
      </ChangelogMarkdown>,
    )

    const link = screen.getByRole('link', { name: 'Release notes' })
    expect(link.getAttribute('target')).toBe('_blank')
    expect(link.getAttribute('rel')).toBe('noreferrer')
    expect(document.querySelector('strong')).toBeNull()
    expect(screen.getByText('Safe')).toBeTruthy()
  })
})
