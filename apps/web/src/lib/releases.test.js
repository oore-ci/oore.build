import { describe, expect, test } from 'bun:test'

import {
  isWaitingOnStore,
  releaseDestinationLabel,
  releaseStageIndex,
  releaseStages,
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
})
