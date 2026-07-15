import { describe, expect, it } from 'vitest'

import { repositoryProjectDefaults } from './project-form-utils'

describe('repositoryProjectDefaults', () => {
  it('uses the repository name and its configured default branch', () => {
    expect(
      repositoryProjectDefaults({
        id: 'repo-1',
        installation_id: 'installation-1',
        external_id: 'mobile/kite',
        full_name: 'mobile-apps/kite',
        default_branch: 'develop',
        is_private: true,
        created_at: 1,
        updated_at: 1,
        integration_id: 'integration-1',
        provider: 'gitlab',
        host_url: 'https://gitlab.example.com',
      }),
    ).toEqual({ name: 'kite', defaultBranch: 'develop' })
  })
})
