import { describe, expect, it, vi } from 'vitest'

import type { Integration, ListRepositoriesResponse } from '@/lib/types'
import { discoverSourceRepositories } from '@/hooks/use-source-repositories'

function integration(id: string): Integration {
  return {
    id,
    provider: 'github',
    host_url: 'https://github.com',
    auth_mode: 'github_app',
    status: 'active',
    created_by: 'user-1',
    created_at: 0,
    updated_at: 0,
  }
}

function repositories(id: string): ListRepositoriesResponse {
  return {
    repositories: [
      {
        id: `repository-${id}`,
        installation_id: `installation-${id}`,
        external_id: id,
        full_name: `owner/${id}`,
        is_private: true,
        created_at: 0,
        updated_at: 0,
      },
    ],
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, reject, resolve }
}

describe('discoverSourceRepositories', () => {
  it('starts repository requests together and retains healthy sources when one fails', async () => {
    const first = deferred<ListRepositoriesResponse>()
    const second = deferred<ListRepositoriesResponse>()
    const third = deferred<ListRepositoriesResponse>()
    const listRepositories = vi.fn((source: Integration) => {
      if (source.id === 'first') return first.promise
      if (source.id === 'second') return second.promise
      return third.promise
    })

    const discovered = discoverSourceRepositories(
      [integration('first'), integration('second'), integration('third')],
      listRepositories,
    )

    expect(listRepositories).toHaveBeenCalledTimes(3)
    first.resolve(repositories('first'))
    second.resolve(repositories('second'))
    third.reject(new Error('Source unavailable'))

    await expect(discovered).resolves.toEqual({
      repositories: [
        expect.objectContaining({
          id: 'repository-first',
          integration_id: 'first',
        }),
        expect.objectContaining({
          id: 'repository-second',
          integration_id: 'second',
        }),
      ],
      failures: [
        expect.objectContaining({
          integration_id: 'third',
          message: 'Source unavailable',
        }),
      ],
    })
  })

  it('returns an empty healthy result with one warning per failed source', async () => {
    const listRepositories = vi.fn((source: Integration) =>
      Promise.reject(new Error(`${source.id} unavailable`)),
    )

    await expect(
      discoverSourceRepositories(
        [integration('first'), integration('second')],
        listRepositories,
      ),
    ).resolves.toEqual({
      repositories: [],
      failures: [
        expect.objectContaining({
          integration_id: 'first',
          message: 'first unavailable',
        }),
        expect.objectContaining({
          integration_id: 'second',
          message: 'second unavailable',
        }),
      ],
    })
  })

  it('does not return discoveries after cancellation', async () => {
    const result = deferred<ListRepositoriesResponse>()
    const controller = new AbortController()
    const listRepositories = vi.fn(() => result.promise)
    const discovered = discoverSourceRepositories(
      [integration('first')],
      listRepositories,
      controller.signal,
    )

    controller.abort(new Error('Cancelled source discovery'))
    result.resolve(repositories('first'))

    await expect(discovered).rejects.toThrow('Cancelled source discovery')
  })
})
