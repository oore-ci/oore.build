import { describe, expect, test } from 'bun:test'

import {
  isWaitingOnStore,
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
    expect(summary.title).toBe(
      'App Store Connect refused the next step for 1.3.3 (33)',
    )
    expect(summary.description).toContain('retries every 5 minutes')
    expect(summary.description).toContain('10m ago')
    // A failed release's last error is history, not a call to act.
    expect(releaseStateLabel({ ...blocked, state: 'failed' })).toBe('Failed')
  })
})
