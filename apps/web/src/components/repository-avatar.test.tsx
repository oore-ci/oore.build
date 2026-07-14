import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import RepositoryAvatar, { repositoryInitials } from './repository-avatar'

describe('RepositoryAvatar', () => {
  it('derives a visible fallback from the repository name', () => {
    render(<RepositoryAvatar fullName="oore-ci/oore.build" />)

    expect(screen.getByText('OO')).toBeTruthy()
    expect(repositoryInitials('oore-ci/oore.build')).toBe('OO')
    expect(repositoryInitials('---')).toBe('R')
  })
})
