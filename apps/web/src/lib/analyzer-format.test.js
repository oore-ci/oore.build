import { expect, test } from 'bun:test'
import {
  analyzerTabCount,
  analyzerTitle,
  formatCountChange,
} from '@/lib/analyzer-format'

test('count changes read as signed numbers', () => {
  expect(formatCountChange(3, 1)).toBe('+2')
  expect(formatCountChange(1, 3)).toBe('−2')
  expect(formatCountChange(2, 2)).toBe('No change')
})

test('the title lists the counts that are not zero, most severe first', () => {
  expect(analyzerTitle({ errors: 0, warnings: 0, infos: 0 })).toBe(
    'No issues found',
  )
  expect(analyzerTitle({ errors: 1, warnings: 0, infos: 2 })).toBe(
    '1 error, 2 infos',
  )
  expect(analyzerTitle({ errors: 0, warnings: 3, infos: 1 })).toBe(
    '3 warnings, 1 info',
  )
})

test('the tab shows errors, then warnings, and nothing for infos only', () => {
  expect(analyzerTabCount({ errors: 2, warnings: 5, infos: 0 })).toBe(
    '2 errors',
  )
  expect(analyzerTabCount({ errors: 0, warnings: 1, infos: 9 })).toBe(
    '1 warning',
  )
  expect(analyzerTabCount({ errors: 0, warnings: 0, infos: 9 })).toBe(null)
})
