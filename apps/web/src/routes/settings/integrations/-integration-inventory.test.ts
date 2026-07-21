import { describe, expect, it } from 'vitest'

import type { IntegrationRepository } from '@/lib/types'
import {
  filterIntegrationRepositories,
  paginateIntegrationRepositories,
} from './-integration-inventory-utils'

function repository(index: number): IntegrationRepository {
  return {
    id: `repository-${index}`,
    installation_id: 'installation-1',
    external_id: String(index),
    full_name: `group/repository-${String(index).padStart(2, '0')}`,
    default_branch: index % 2 === 0 ? 'main' : 'develop',
    is_private: true,
    created_at: 1,
    updated_at: 1,
  }
}

describe('integration repository inventory', () => {
  const repositories = Array.from({ length: 31 }, (_, index) =>
    repository(index + 1),
  )

  it('keeps a 31-repository source bounded to the requested page', () => {
    const filtered = filterIntegrationRepositories(repositories, undefined)

    expect(paginateIntegrationRepositories(filtered, 1, 20)).toHaveLength(20)
    const secondPage = paginateIntegrationRepositories(filtered, 2, 20)
    expect(secondPage).toHaveLength(11)
    expect(secondPage[0]?.full_name).toBe('group/repository-21')
  })

  it('filters repositories by search text', () => {
    expect(
      filterIntegrationRepositories(repositories, 'repository-30').map(
        (item) => item.full_name,
      ),
    ).toEqual(['group/repository-30'])

    expect(filterIntegrationRepositories(repositories, 'develop')).toHaveLength(
      16,
    )
  })
})
