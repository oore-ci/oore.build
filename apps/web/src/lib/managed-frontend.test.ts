import { describe, expect, it, vi } from 'vitest'

import { isManagedFrontend } from './managed-frontend'

describe('isManagedFrontend', () => {
  it('recognizes the same-origin oore-web proxy', async () => {
    const fetcher = vi.fn(() =>
      Promise.resolve(
        new Response('{}', {
          status: 200,
          headers: { 'x-oore-web-proxy': '1' },
        }),
      ),
    )
    await expect(isManagedFrontend(fetcher)).resolves.toBe(true)
  })

  it('does not claim a generic or unavailable frontend', async () => {
    await expect(
      isManagedFrontend(vi.fn(() => Promise.resolve(new Response('{}')))),
    ).resolves.toBe(false)
    await expect(
      isManagedFrontend(vi.fn(() => Promise.reject(new Error('down')))),
    ).resolves.toBe(false)
  })
})
