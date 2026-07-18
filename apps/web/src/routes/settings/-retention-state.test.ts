import { describe, expect, it } from 'vitest'

import { resolveRetentionPolicyLoadState } from './-retention-state'

describe('retention policy load state', () => {
  it('does not treat a failed policy query as ready', () => {
    expect(resolveRetentionPolicyLoadState(false, new Error('offline'))).toBe(
      'error',
    )
  })

  it('keeps loading and ready states distinct', () => {
    expect(resolveRetentionPolicyLoadState(true, null)).toBe('loading')
    expect(resolveRetentionPolicyLoadState(false, null)).toBe('ready')
  })
})
