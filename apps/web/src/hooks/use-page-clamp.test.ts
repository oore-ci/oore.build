import { describe, expect, it } from 'vitest'

import { clampCollectionPage } from './use-page-clamp'

describe('clampCollectionPage', () => {
  it('keeps valid pages and clamps stale URL pages', () => {
    expect(clampCollectionPage(2, 20, 45)).toBe(2)
    expect(clampCollectionPage(999, 20, 45)).toBe(3)
  })

  it('uses page one for an empty collection', () => {
    expect(clampCollectionPage(4, 20, 0)).toBe(1)
  })
})
