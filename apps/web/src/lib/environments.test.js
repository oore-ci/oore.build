import { describe, expect, test } from 'bun:test'

import {
  approverMode,
  approversInput,
  approversSummary,
  formatRolloutSteps,
  nextEnvironment,
  parseRolloutSteps,
  playTrackStagesRollouts,
  rolloutPolicySummary,
  trackNextAction,
  trackRows,
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

  test('closed and custom Play tracks never stage', () => {
    for (const track of ['closed', 'qa-team']) {
      expect(
        rolloutPolicySummary({ track, rollout_policy: { steps: [0.1, 1] } }),
      ).toBe('Full release')
    }
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

  test('App Store versions use phased release and TestFlight never stages', () => {
    const steps = { steps: [0.1, 1] }
    expect(
      rolloutPolicySummary({
        store: 'app_store',
        track: 'app_store',
        rollout_policy: steps,
      }),
    ).toBe("Phased release on Apple's 7-day schedule")
    expect(
      rolloutPolicySummary({
        store: 'app_store',
        track: 'app_store',
        rollout_policy: { steps: [1] },
      }),
    ).toBe('Full release')
    expect(
      rolloutPolicySummary({
        store: 'app_store',
        track: 'External',
        rollout_policy: steps,
      }),
    ).toBe('TestFlight, no staged rollout')
  })
})

describe('nextEnvironment', () => {
  test('skips environments builds cannot be promoted into', () => {
    const environments = [
      { id: 'a', promotable: true, store: 'google_play' },
      { id: 'b', promotable: false, store: 'google_play' },
      { id: 'c', promotable: true, store: 'google_play' },
    ]
    expect(nextEnvironment(environments, environments[0])?.id).toBe('c')
    expect(nextEnvironment(environments, environments[2])).toBeUndefined()
  })

  test('stays in the store the build was released to', () => {
    const environments = [
      { id: 'internal', promotable: true, store: 'google_play' },
      { id: 'testflight', promotable: true, store: 'app_store' },
      { id: 'production', promotable: true, store: 'google_play' },
      { id: 'app-store', promotable: true, store: 'app_store' },
    ]
    expect(nextEnvironment(environments, environments[0])?.id).toBe(
      'production',
    )
    expect(nextEnvironment(environments, environments[1])?.id).toBe('app-store')
  })
})

describe('approvers', () => {
  const ana = { user_id: 'u1', email: 'ana@example.com' }

  test('summarises who can approve', () => {
    expect(approversSummary(undefined)).toBe('Maintainers')
    expect(approversSummary({ users: [] })).toBe('Maintainers')
    expect(approversSummary({ role: 'developer', users: [ana] })).toBe(
      'Developers and maintainers, ana@example.com',
    )
    expect(approversSummary({ role: 'maintainer', users: [ana] })).toBe(
      'Maintainers, ana@example.com',
    )
    expect(approversSummary({ users: [ana] })).toBe('ana@example.com')
  })

  test('round-trips the dialog mode', () => {
    expect(approverMode(undefined)).toBe('maintainer')
    expect(approverMode({ role: 'maintainer', users: [ana] })).toBe(
      'maintainer',
    )
    expect(approverMode({ role: 'developer', users: [] })).toBe('developer')
    expect(approverMode({ users: [ana] })).toBe('named')
    expect(approversInput('maintainer', [])).toEqual({})
    expect(approversInput('maintainer', ['u1'])).toEqual({
      role: 'maintainer',
      user_ids: ['u1'],
    })
    expect(approversInput('developer', [])).toEqual({
      role: 'developer',
      user_ids: [],
    })
    expect(approversInput('named', ['u1'])).toEqual({ user_ids: ['u1'] })
  })
})

describe('playTrackStagesRollouts', () => {
  test('only production and open testing stage', () => {
    for (const track of ['production', 'open', 'beta', ' open ']) {
      expect(playTrackStagesRollouts(track)).toBe(true)
    }
    for (const track of ['internal', 'closed', 'alpha', 'qa-team']) {
      expect(playTrackStagesRollouts(track)).toBe(false)
    }
  })
})

describe('trackRows and trackNextAction', () => {
  const env = (id, track, position, extra = {}) => ({
    id,
    name: id,
    store: 'google_play',
    track,
    position,
    promotable: true,
    approval_required: false,
    ...extra,
  })
  const release = (id, track, state) => ({
    id,
    store: 'google_play',
    track,
    state,
    created_at: 1,
    allowed_actions: [],
  })
  const primary = (r) =>
    r.state === 'rolling' ? 'halt' : r.state === 'halted' ? 'resume' : null

  test('lists set-up tracks in order, then store tracks without setup', () => {
    const rows = trackRows(
      [env('beta', 'open', 2), env('internal', 'internal', 1)],
      [
        {
          key: 'google_play:custom',
          store: 'google_play',
          track: 'custom',
          out: null,
          coming: [],
        },
        {
          key: 'google_play:open',
          store: 'google_play',
          track: 'open',
          out: release('r1', 'open', 'complete'),
          coming: [],
        },
      ],
      [{ id: 'p', state: 'pending', environment_id: 'internal' }],
    )
    expect(rows.map((row) => row.key)).toEqual([
      'google_play:internal',
      'google_play:open',
      'google_play:custom',
    ])
    expect(rows[0].pending?.id).toBe('p')
    expect(rows[1].out?.id).toBe('r1')
    expect(rows[2].environment).toBeUndefined()
  })

  test('approve leads, then resume or halt, then promote', () => {
    const pending = { id: 'p', can_decide: true }
    const rolling = release('r', 'production', 'rolling')
    expect(
      trackNextAction({ out: rolling, coming: [], pending }, true, primary),
    ).toEqual({ kind: 'approve', promotion: pending })
    expect(
      trackNextAction(
        { out: rolling, coming: [], pending: { id: 'q', can_decide: false } },
        true,
        primary,
      ),
    ).toEqual({ kind: 'release', release: rolling })
    const done = release('d', 'internal', 'complete')
    expect(trackNextAction({ out: done, coming: [] }, true, primary)).toEqual({
      kind: 'promote',
    })
    expect(trackNextAction({ out: done, coming: [] }, false, primary)).toBe(
      null,
    )
  })
})
