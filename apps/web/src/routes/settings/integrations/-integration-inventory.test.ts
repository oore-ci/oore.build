import { describe, expect, it } from 'vitest'

import type { IntegrationRepository } from '@/lib/types'
import {
  filterIntegrationRepositories,
  paginateIntegrationRepositories,
} from './-integration-inventory-utils'

function repository(
  index: number,
  allowDirectRunner = false,
): IntegrationRepository {
  return {
    id: `repository-${index}`,
    installation_id: 'installation-1',
    external_id: String(index),
    full_name: `group/repository-${String(index).padStart(2, '0')}`,
    default_branch: index % 2 === 0 ? 'main' : 'develop',
    is_private: true,
    allow_direct_macos_runner: allowDirectRunner,
    created_at: 1,
    updated_at: 1,
  }
}

describe('integration repository inventory', () => {
  const repositories = Array.from({ length: 31 }, (_, index) =>
    repository(index + 1, (index + 1) % 3 === 0),
  )

  it('keeps a 31-repository source bounded to the requested page', () => {
    const filtered = filterIntegrationRepositories(
      repositories,
      undefined,
      'all',
    )

    expect(paginateIntegrationRepositories(filtered, 1, 20)).toHaveLength(20)
    const secondPage = paginateIntegrationRepositories(filtered, 2, 20)
    expect(secondPage).toHaveLength(11)
    expect(secondPage[0]?.full_name).toBe('group/repository-21')
  })

  it('uses compact ten-row pages without dropping repositories', () => {
    const filtered = filterIntegrationRepositories(
      repositories,
      undefined,
      'all',
    )

    expect(paginateIntegrationRepositories(filtered, 1, 10)).toHaveLength(10)
    expect(paginateIntegrationRepositories(filtered, 3, 10)).toHaveLength(10)
    expect(paginateIntegrationRepositories(filtered, 4, 10)).toHaveLength(1)
  })

  it('combines search and runner access filters', () => {
    expect(
      filterIntegrationRepositories(
        repositories,
        'repository-30',
        'allowed',
      ).map((item) => item.full_name),
    ).toEqual(['group/repository-30'])

    expect(
      filterIntegrationRepositories(repositories, 'develop', 'blocked'),
    ).toHaveLength(11)
  })
})
