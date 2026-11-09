import { describe, expect, test } from 'bun:test'

import {
  isWaitingOnStore,
  latestChangePerRelease,
  releaseChangeDestination,
  releaseChangeShare,
  releaseDestinationLabel,
  releaseStageIndex,
  releaseNextStep,
  releaseStages,
  releaseStateBadgeVariant,
  releaseStateLabel,
  releaseStatusPhrase,
  releaseWaitSummary,
} from './releases'

const appStore = {
  store: 'app_store',
  track: 'app_store',
  state: 'processing',
  awaiting_approval: false,
  version_name: '1.3.3',
  version_code: 33,
  created_at: Math.floor(Date.now() / 1000) - 240,
  updated_at: Math.floor(Date.now() / 1000) - 240,
}

describe('store wait', () => {
  test('a processing upload is still waiting on the store', () => {
    expect(isWaitingOnStore(appStore)).toBe(true)
    expect(isWaitingOnStore({ ...appStore, state: 'rolling' })).toBe(false)
    expect(
      isWaitingOnStore({
        ...appStore,
        state: 'rolling',
        awaiting_approval: true,
      }),
    ).toBe(true)
  })

  test('names the destination in the store’s words', () => {
    expect(releaseDestinationLabel(appStore)).toBe('App Store')
    expect(releaseDestinationLabel({ ...appStore, track: 'QA, Beta' })).toBe(
      'TestFlight · QA, Beta',
    )
    expect(
      releaseDestinationLabel({ store: 'google_play', track: 'production' }),
    ).toBe('Google Play · production')
    expect(releaseStatusPhrase(appStore)).toBe('App Store processing')
    expect(releaseStatusPhrase({ ...appStore, track: 'QA' })).toBe(
      'TestFlight processing',
    )
  })

  test('places each state on its stage', () => {
    expect(releaseStages(appStore)[releaseStageIndex(appStore)]).toBe(
      'Apple processing',
    )
    const review = { ...appStore, state: 'in_review' }
    expect(releaseStages(review)[releaseStageIndex(review)]).toBe(
      'App Store review',
    )
    const play = {
      store: 'google_play',
      track: 'production',
      state: 'in_review',
    }
    expect(releaseStages(play)[releaseStageIndex(play)]).toBe('Review')
  })

  test('says Apple is processing and what Oore does next', () => {
    const summary = releaseWaitSummary(appStore)
    expect(summary.title).toBe('Uploaded. Apple is processing the build')
    expect(summary.description).toContain('1.3.3 (33)')
    expect(summary.description).toContain('4m ago')
    expect(summary.description).toContain('App Store version 1.3.3')
    expect(
      releaseWaitSummary({ ...appStore, track: 'QA' }).description,
    ).toContain('available to QA in TestFlight')
    expect(releaseWaitSummary({ ...appStore, state: 'rolling' })).toBeNull()
  })

  test('says what Apple says while it reviews', () => {
    const waiting = {
      ...appStore,
      state: 'in_review',
      store_status: 'WAITING_FOR_REVIEW',
    }
    expect(releaseStateLabel(waiting)).toBe('Waiting for review')
    expect(releaseStatusPhrase(waiting)).toBe('App Store waiting for review')
    expect(releaseNextStep(waiting, 0)).toBe(
      "In Apple's queue; review hasn't started",
    )
    expect(releaseWaitSummary(waiting).title).toBe(
      'Waiting for App Store review',
    )

    const reviewing = { ...waiting, store_status: 'IN_REVIEW' }
    expect(releaseStateLabel(reviewing)).toBe('In review')
    expect(releaseWaitSummary(reviewing).title).toBe(
      'Apple is reviewing the app',
    )

    // Without Apple's status, fall back to Oore's own state.
    expect(releaseStateLabel({ ...appStore, state: 'in_review' })).toBe(
      'In review',
    )
    // A finished release no longer shows a stale store status.
    expect(releaseStateLabel({ ...reviewing, state: 'complete' })).toBe(
      'Complete',
    )
  })

  test('a store refusal needs attention, not more waiting', () => {
    const blocked = {
      ...appStore,
      store_error:
        'STATE_ERROR.ENTITY_STATE_INVALID: This resource cannot be reviewed\n- An age rating declaration update is required',
      store_error_since: Math.floor(Date.now() / 1000) - 600,
    }
    expect(isWaitingOnStore({ ...blocked, state: 'rolling' })).toBe(true)
    expect(releaseStateLabel(blocked)).toBe('Needs attention')
    expect(releaseStateBadgeVariant(blocked)).toBe('warning')
    expect(releaseStatusPhrase(blocked)).toBe('App Store needs attention')
    expect(releaseNextStep(blocked, 0)).toBe(
      'Fix the issue in App Store Connect',
    )
    const summary = releaseWaitSummary(blocked)
    expect(summary.blocked).toBe(true)
    expect(summary.title).toBe('1.3.3 (33) is blocked in App Store Connect')
    expect(summary.description).toContain('every 5 minutes')
    expect(summary.description).toContain('10m ago')
    // A failed release's last error is history, not a call to act.
    expect(releaseStateLabel({ ...blocked, state: 'failed' })).toBe('Failed')
  })

  test('a blocked release says so and why, and sorts nothing as in review', () => {
    const refused = {
      ...appStore,
      state: 'blocked',
      state_reason:
        'App Store Connect refused the request: an age rating declaration update is required',
    }
    expect(releaseStateLabel(refused)).toBe('Blocked')
    expect(releaseStateBadgeVariant(refused)).toBe('warning')
    expect(isWaitingOnStore(refused)).toBe(true)
    expect(releaseNextStep(refused, 0)).toBe('Fix it in App Store Connect')
    expect(releaseWaitSummary(refused).description).toContain('age rating')
    const rejected = {
      ...appStore,
      state: 'rejected',
      state_reason: 'Guideline 2.1',
    }
    expect(releaseStateBadgeVariant(rejected)).toBe('destructive')
    expect(releaseWaitSummary(rejected).title).toBe('Apple rejected 1.3.3 (33)')
    const approved = {
      ...appStore,
      state: 'approved',
      store_status: 'PENDING_DEVELOPER_RELEASE',
    }
    expect(releaseNextStep(approved, 0)).toBe('Approved; release it when ready')
    expect(releaseWaitSummary(approved).description).toContain('Release now')
  })
})

describe('latestChangePerRelease', () => {
  const step = (release_id, at, to_state, to_fraction, extra = {}) => ({
    id: `${release_id}-${at}`,
    release_id,
    at,
    to_state,
    to_fraction,
    project_id: 'p1',
    project_name: 'Kite',
    store: 'google_play',
    track: 'production',
    version_name: release_id,
    ...extra,
  })

  test('keeps one row per app version, newest change first', () => {
    const rows = latestChangePerRelease(
      [
        step('1.3.3', 50, 'complete', 1, {
          store: 'app_store',
          track: 'app_store',
        }),
        step('1.3.3', 40, 'rolling', 0.5, {
          store: 'app_store',
          track: 'app_store',
        }),
        step('2.0.0', 60, 'in_review', null),
        step('1.3.3', 20, 'rolling', 0.1, {
          store: 'app_store',
          track: 'app_store',
        }),
      ],
      5,
    )
    expect(
      rows.map((row) => [row.event.version_name, row.event.at, row.changes]),
    ).toEqual([
      ['2.0.0', 60, 1],
      ['1.3.3', 50, 3],
    ])
  })

  test('folds one version on several tracks into one row, newer version first on a tie', () => {
    const rows = latestChangePerRelease(
      [
        step('3.11.11', 10, 'complete', 1, { track: 'TradingQnA' }),
        step('3.11.13', 10, 'complete', 1, { track: 'internal' }),
        step('3.11.11', 10, 'complete', 1, { track: 'alpha' }),
        step('3.11.13', 10, 'complete', 1, { track: 'beta' }),
        step('3.11.13', 10, 'complete', 1, { track: 'production' }),
      ],
      5,
    )
    expect(
      rows.map((row) => [
        row.event.version_name,
        releaseChangeDestination(row),
      ]),
    ).toEqual([
      ['3.11.13', 'Google Play · internal, beta, production'],
      ['3.11.11', 'Google Play · TradingQnA, alpha'],
    ])
  })

  test('stops at the limit', () => {
    const rows = latestChangePerRelease(
      [
        step('a', 3, 'complete', 1),
        step('b', 2, 'complete', 1),
        step('c', 1, 'complete', 1),
      ],
      2,
    )
    expect(rows.map((row) => row.event.version_name)).toEqual(['a', 'b'])
  })

  test('names the share only while a rollout is partial', () => {
    expect(releaseChangeShare({ to_state: 'rolling', to_fraction: 0.2 })).toBe(
      '20%',
    )
    expect(
      releaseChangeShare({ to_state: 'complete', to_fraction: 1 }),
    ).toBeNull()
    expect(
      releaseChangeShare({ to_state: 'in_review', to_fraction: null }),
    ).toBeNull()
  })
})
