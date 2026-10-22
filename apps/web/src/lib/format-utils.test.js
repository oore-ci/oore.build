import { describe, expect, test } from 'bun:test'

import { formatDuration } from './format-utils'

describe('formatDuration', () => {
  test('rounds total seconds before splitting minutes', () => {
    expect(formatDuration(119.6)).toBe('2m 0s')
    expect(formatDuration(59.6)).toBe('1m 0s')
  })

  test('clamps clock skew at zero', () => {
    expect(formatDuration(-4)).toBe('0s')
  })
})
