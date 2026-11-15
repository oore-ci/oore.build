import { describe, expect, test } from 'bun:test'

import { describeStoreUploads, playTrackLabel } from './store-destination'

const play = {
  enabled: true,
  track: 'production',
  rollout_percent: 5,
  draft: false,
}
const appStore = {
  enabled: true,
  destination: 'testflight',
  testflight_groups: ['Internal QA'],
  submit_for_review: false,
}

describe('describeStoreUploads', () => {
  test('names the track and the share', () => {
    expect(describeStoreUploads({ googlePlay: play })).toBe(
      'Uploads to Google Play Production at 5% after each build.',
    )
  })

  test('joins both stores', () => {
    expect(
      describeStoreUploads({
        googlePlay: { ...play, track: 'internal', rollout_percent: null },
        appStore,
      }),
    ).toBe(
      'Uploads to Google Play Internal testing and TestFlight (Internal QA) after each build.',
    )
  })

  test('says so when nothing uploads', () => {
    expect(
      describeStoreUploads({ googlePlay: { ...play, enabled: false } }),
    ).toBe('Builds are not uploaded to a store.')
    expect(describeStoreUploads({})).toBe('Builds are not uploaded to a store.')
  })

  test('keeps custom track names and drafts', () => {
    expect(
      describeStoreUploads({
        googlePlay: { ...play, track: 'qa-team', draft: true },
      }),
    ).toBe(
      'Uploads to the Google Play qa-team track as a draft after each build.',
    )
  })
})

describe('playTrackLabel', () => {
  test('plain names for standard tracks', () => {
    expect(playTrackLabel('closed')).toBe('Closed testing')
    expect(playTrackLabel('qa-team')).toBe('qa-team')
  })
})
