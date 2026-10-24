import { describe, expect, test } from 'bun:test'

import {
  formatRolloutSteps,
  nextEnvironment,
  parseRolloutSteps,
  rolloutPolicySummary,
} from './environments'

describe('parseRolloutSteps', () => {
  test('reads percentages as fractions', () => {
    expect(parseRolloutSteps('1, 5, 20%, 100')).toEqual({
      steps: [0.01, 0.05, 0.2, 1],
    })
    expect(parseRolloutSteps('100')).toEqual({ steps: [1] })
  })

  test('refuses steps the daemon would reject', () => {
    expect(parseRolloutSteps('')).toHaveProperty('error')
    expect(parseRolloutSteps('5, 1, 100')).toHaveProperty('error')
    expect(parseRolloutSteps('1, 50')).toHaveProperty('error')
    expect(parseRolloutSteps('0, 100')).toHaveProperty('error')
    expect(parseRolloutSteps('50, 150')).toHaveProperty('error')
  })

  test('round-trips what the form shows', () => {
    const steps = [0.005, 0.2, 1]
    expect(parseRolloutSteps(formatRolloutSteps(steps))).toEqual({ steps })
  })
})

describe('rolloutPolicySummary', () => {
  test('internal testing never stages', () => {
    expect(
      rolloutPolicySummary({
        track: 'internal',
        rollout_policy: { steps: [0.1, 1] },
      }),
    ).toBe('Full release')
  })

  test('describes the schedule and the crash-rate halt', () => {
    expect(
      rolloutPolicySummary({
        track: 'production',
        rollout_policy: {
          steps: [0.05, 0.5, 1],
          interval_hours: 12,
          crash_rate_threshold: 0.02,
        },
      }),
    ).toBe('5% → 50% → 100% every 12h, halts at 2% crashes')
  })
})

describe('nextEnvironment', () => {
  test('skips environments builds cannot be promoted into', () => {
    const environments = [
      { id: 'a', promotable: true },
      { id: 'b', promotable: false },
      { id: 'c', promotable: true },
    ]
    expect(nextEnvironment(environments, environments[0])?.id).toBe('c')
    expect(nextEnvironment(environments, environments[2])).toBeUndefined()
  })
})
