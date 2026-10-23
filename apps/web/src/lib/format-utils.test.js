import { describe, expect, test } from 'bun:test'

import { formatDuration, formatSizeChange } from './format-utils'

describe('formatDuration', () => {
  test('rounds total seconds before splitting minutes', () => {
    expect(formatDuration(119.6)).toBe('2m 0s')
    expect(formatDuration(59.6)).toBe('1m 0s')
  })

  test('clamps clock skew at zero', () => {
    expect(formatDuration(-4)).toBe('0s')
  })
})

describe('formatSizeChange', () => {
  test('signs the change and its share of the baseline', () => {
    expect(formatSizeChange(1_258_291, 24_222_105)).toBe('+1.2 MB (+5.2%)')
    expect(formatSizeChange(-300, 300_000)).toBe('-300 B (-0.1%)')
  })

  test('says when nothing changed or there is no baseline size', () => {
    expect(formatSizeChange(0, 10)).toBe('No change')
    expect(formatSizeChange(2_048, 0)).toBe('+2.0 KB')
  })
})
