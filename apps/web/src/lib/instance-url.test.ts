import { describe, expect, it } from 'vitest'

import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'

describe('instance URL resolution', () => {
  it('uses explicit backend URLs as-is without trailing slashes', () => {
    expect(
      resolveInstanceApiBaseUrl({ url: 'https://ci.example.com///' }),
    ).toBe('https://ci.example.com')
  })

  it('uses the current origin for empty local-proxy instances', () => {
    const previousWindow = globalThis.window
    Object.defineProperty(globalThis, 'window', {
      configurable: true,
      value: { location: { origin: 'https://oore.example.com' } },
    })

    expect(resolveInstanceApiBaseUrl({ url: '' })).toBe(
      'https://oore.example.com',
    )
    expect(resolveInstanceApiBaseUrl({ url: 'local' })).toBe(
      'https://oore.example.com',
    )

    Object.defineProperty(globalThis, 'window', {
      configurable: true,
      value: previousWindow,
    })
  })
})
