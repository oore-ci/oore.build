import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import RepositoryAvatar from './repository-avatar'
import { repositoryInitials } from '@/lib/repository-avatar'

const { mockUseRepositoryAvatarUrl } = vi.hoisted(() => ({
  mockUseRepositoryAvatarUrl: vi.fn(),
}))

vi.mock('@/hooks/use-repository-avatar-url', () => ({
  useRepositoryAvatarUrl: mockUseRepositoryAvatarUrl,
}))

describe('RepositoryAvatar', () => {
  it('derives a visible fallback from the repository name', () => {
    render(<RepositoryAvatar fullName="oore-ci/oore.build" />)

    expect(screen.getByText('OO')).toBeTruthy()
    expect(repositoryInitials('oore-ci/oore.build')).toBe('OO')
    expect(repositoryInitials('---')).toBe('R')
  })

  it('uses Oore for GitLab avatars', () => {
    mockUseRepositoryAvatarUrl.mockReturnValue('blob:gitlab-avatar')

    render(
      <RepositoryAvatar
        fullName="oore-ci/oore.build"
        avatarUrl="https://gitlab.example/uploads/avatar.png"
        repositoryId="repo-1"
        provider="gitlab"
      />,
    )

    expect(mockUseRepositoryAvatarUrl).toHaveBeenCalledWith('repo-1', true)
  })
})
