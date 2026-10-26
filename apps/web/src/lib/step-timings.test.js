import assert from 'node:assert/strict'
import { test } from 'bun:test'

import {
  formatStepDelta,
  longestStepMs,
  stepMedians,
  stepTimingSummary,
  stepTimingTrend,
} from './step-timings'

const build = (id, steps, extra = {}) => ({
  id,
  ...extra,
  step_results: steps.map(([name, seconds, status = 'succeeded']) => ({
    name,
    status,
    duration_ms: seconds * 1000,
  })),
})

test('step medians skip the current build, failed steps and thin samples', () => {
  const medians = stepMedians(
    [
      build('current', [['build', 900]]),
      build('a', [
        ['build', 300],
        ['test', 60],
      ]),
      build('b', [
        ['build', 100],
        ['test', 80],
      ]),
      build('c', [
        ['build', 200],
        ['test', 999, 'failed'],
      ]),
      build('d', [['build', 400]]),
      build('retry', [['build', 5]], {
        source_build_id: 'a',
        retry_steps: ['build'],
      }),
      build('rerun', [['build', 5]], { source_build_id: 'b' }),
    ],
    'current',
  )
  // Even count: the mean of the two middle runs.
  assert.equal(medians.get('build'), 250_000)
  // Two successful runs are too few to compare with.
  assert.equal(medians.has('test'), false)
})

test('a step is slower or faster only when the gap is meaningful', () => {
  assert.equal(stepTimingTrend(130_000, 100_000), 'slower')
  assert.equal(stepTimingTrend(70_000, 100_000), 'faster')
  // Within 20%.
  assert.equal(stepTimingTrend(115_000, 100_000), 'typical')
  // 50% slower but only 3 seconds.
  assert.equal(stepTimingTrend(9_000, 6_000), 'typical')
})

test('bars share one scale set by the slowest step or median', () => {
  const steps = [
    { name: 'build', durationMs: 240_000 },
    { name: 'test', durationMs: 90_000 },
    { name: 'running' },
  ]
  assert.equal(longestStepMs(steps), 240_000)
  assert.equal(longestStepMs(steps, new Map([['build', 300_000]])), 300_000)
  assert.equal(longestStepMs([{ name: 'queued' }]), 0)
})

test('step summaries show time and median delta, or line count without a time', () => {
  assert.equal(formatStepDelta(98_000, 60_000), '+38s')
  assert.equal(formatStepDelta(180_000, 240_000), '−1m 0s')
  assert.equal(formatStepDelta(60_400, 60_000), '±0s')
  assert.equal(stepTimingSummary(240_000, 300_000, 11), '4m 0s (−1m 0s)')
  assert.equal(stepTimingSummary(240_000, undefined, 11), '4m 0s')
  assert.equal(stepTimingSummary(undefined, 300_000, 1), '1 line')
  assert.equal(stepTimingSummary(undefined, undefined, 32), '32 lines')
})
