import { expect, test } from 'bun:test'
import { enabledStatus, getStatusPresentation } from '@/lib/status-variants'
import { formatEnumLabel } from '@/lib/format-utils'

test('formatEnumLabel turns machine enums into sentence case', () => {
  expect(formatEnumLabel('timed_out')).toBe('Timed out')
  expect(formatEnumLabel('pull_request')).toBe('Pull request')
  expect(formatEnumLabel('tag-push')).toBe('Tag push')
  expect(formatEnumLabel('  running ')).toBe('Running')
  expect(formatEnumLabel('')).toBe('')
})

test('every status kind renders a sentence-case label, never the raw enum', () => {
  expect(getStatusPresentation('build', 'timed_out')).toEqual({
    label: 'Timed out',
    variant: 'outline',
  })
  expect(getStatusPresentation('build', 'succeeded').variant).toBe('success')
  expect(getStatusPresentation('build', 'failed').variant).toBe('destructive')
  expect(getStatusPresentation('runner', 'draining')).toEqual({
    label: 'Draining',
    variant: 'warning',
  })
  expect(getStatusPresentation('integration', 'error')).toEqual({
    label: 'Error',
    variant: 'destructive',
  })
  expect(getStatusPresentation('user', 'invited')).toEqual({
    label: 'Invited',
    variant: 'outline',
  })
  expect(getStatusPresentation('delivery', 'delivered')).toEqual({
    label: 'Delivered',
    variant: 'secondary',
  })
  expect(getStatusPresentation('enabled', enabledStatus(false))).toEqual({
    label: 'Disabled',
    variant: 'outline',
  })
})

test('unknown values fall back to a formatted label and neutral variant', () => {
  expect(getStatusPresentation('runner', 'some_new_state')).toEqual({
    label: 'Some new state',
    variant: 'outline',
  })
})
