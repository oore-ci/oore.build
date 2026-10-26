import { expect, test } from 'bun:test'
import { ALL_PLATFORMS, effectivePlanSelection } from '@/lib/pipeline-plan'

test('a chosen platform or workflow applies while it is offered', () => {
  expect(
    effectivePlanSelection({
      platform: 'ios',
      platforms: ['android', 'ios'],
      workflow: 'release',
      workflows: ['nightly', 'release'],
    }),
  ).toEqual({ platform: 'ios', workflow: 'release' })
})

test('a choice the configuration no longer offers falls back to the defaults', () => {
  expect(
    effectivePlanSelection({
      platform: 'ios',
      platforms: ['android'],
      workflow: 'release',
      workflows: ['nightly'],
    }),
  ).toEqual({ platform: undefined, workflow: undefined })
  expect(
    effectivePlanSelection({
      platform: ALL_PLATFORMS,
      platforms: ['android', 'ios'],
      workflow: null,
      workflows: [],
    }),
  ).toEqual({ platform: undefined, workflow: undefined })
})
